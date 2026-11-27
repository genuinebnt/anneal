# Build your own: series, parts and capstones

A catalogue to build from, not a promise that these exist. **Written problems:** the six in section 9 are new, staged outside the repository; everything marked *to be written* has no problem yet. Times are **my estimates** for a solid Rust developer who has done the required parts. None was measured. Difficulty and priority (P1 first, P3 last) are my judgement. No claim is made that any company asks any of these.

How to read a part: **Core** fits the first stretch; **follow-ups** are what an interviewer adds, each harder, each with how it would be tested. Cookbook links come in three levels. **exact**: the page's code was read for this catalogue and solves the core (the note says what it lacks). **similar** or **related**: same technique, or the idea explained. **similar\*** means the page's headings or question list suggest it solves the task, but I did not read that code, so it is not labelled exact. Links that would give away the method are folded under *hidden until asked*. Nothing is locked: every part is meant to start from its own starter code. Overlap between parts is deliberate; they differ in what you build.

Contents: 1 coverage matrix · 2 counts and how thorough · 3 series overview · 4 roadmap, concept map, transfer chains · 5 series (the main catalogue) · 6 views by time box · 7 capstones and readiness · 8 what the runner cannot grade · 9 the six written problems · 10 series in the app

## 1. Coverage matrix

Rows come from four outlines checked on this machine: the chapter list of the Rust Reference and the module list of `std` (the local `rust-docs` of rustc 1.98.1), the Cookbook outline (`tools/cookbook/outline.json`) and the tracks planned in `docs/CURRICULUM.md`. It is a topic-level match, not a section-by-section reading of the Reference. Existing ids are examples, not every match.


### Language (Reference chapters; CURRICULUM section L)

| Topic | Outline | Exercises (series part) | Existing | Staged drafts | Gap or reason |
|---|---|---|---|---|---|
| Ownership and moves | Ref: statements and expressions, destructors; L1 | split-borrow-bank (OWN 1), linked-list-safe (OWN 2), undo-stack (OWN 4) | `l1-fix-use-after-move`, `l1-fix-partial-moves`, `l1-ownership-round-trip`, `l1-fix-needless-clones` | - |  |
| Borrowing and borrow checker limits | Ref: expressions (place); L2 | split-borrow-bank (OWN 1), view-splitter (OWN 3), graph-ownership (OWN 6), alg-ast-walk (REC 2) | `l2-get-disjoint-mut`, `l2-fix-get-or-insert`, `l2-fix-conditional-return-of-borrow`, `l2-view-struct` | - |  |
| Lifetimes and elision | Ref: lifetime elision; L3 | view-splitter (OWN 3), tokenizer-borrowed (PARSE 2), url-parser (PARSE 6), scoped-callbacks (OWN 7) | `l3-zero-copy-tokenizer`, `l3-why-a-everywhere-fails`, `l3-self-referential-parser` | - |  |
| Variance | Ref: subtyping and variance; L3 | scoped-callbacks (OWN 7), intrusive-list (UNSAFE 6) | `l3-fix-mut-invariance` | - | Variance is mostly visible at compile time. Behaviour tests cannot grade a wrong variance in a learner's type; only existing fix-mode problems can. |
| HRTB and GATs | Ref: trait bounds; L3, L4 | scoped-callbacks (OWN 7), lending-windows (OWN 8), parser-combinators (PARSE 16) | `l3-hrtb-borrowed-iteration`, `l4-fix-hrtb-decode-owned`, `l4-lending-iterator` | - |  |
| Types, layout of enums and structs, DSTs | Ref: types, dynamically sized types, type layout | layout-quiz-codec (UNSAFE 1), units-newtypes (TRAIT 2), heterogeneous-store (TRAIT 4) | `f2-four-byte-optional-index`, `f2-eight-byte-instructions`, `f2-box-the-large-variant`, `s1-fix-option-niche` | - |  |
| Coercions, casts and conversions | Ref: type coercions; std convert; S8 | units-newtypes (TRAIT 2), semver (PARSE 5) | `l2-explicit-reborrow-generic-sink`, `s1-as-ref-as-mut-as-deref` | - | S8 (core traits) is unwritten: AsRef, Borrow, TryFrom drills are still thin. |
| Traits, default and blanket impls, coherence | Ref: items (traits, impls); special traits; L4 | shape-registry (TRAIT 1), generic-matrix (TRAIT 3), library-api (TRAIT 8) | `l4-blanket-impls`, `l4-fix-orphan-rule`, `l4-fix-conflicting-impls`, `l4-default-methods` | - |  |
| Generics, bounds, associated types, const generics | Ref: items (generics); L5 | generic-matrix (TRAIT 3), range-queries (COLL 12), units-newtypes (TRAIT 2) | `l5-associated-type-vs-parameter`, `l5-shape-typed-matrix`, `l5-inner-fn-monomorphization` | - |  |
| Static and dynamic dispatch, object safety | Ref: trait objects; L4 | shape-registry (TRAIT 1), fn-pipeline (ITER 2), manual-vtable (TRAIT 9), async-trait-object (ASYNC 11) | `l4-fix-generic-method-dyn`, `l4-fix-self-in-argument`, `l4-shapes-both-ways` | - |  |
| Closures and Fn traits | Ref: closure expressions and types; L6 | fn-pipeline (ITER 2), memoize (ITER 3), scoped-callbacks (OWN 7) | `l1-fix-closure-may-outlive`, `l2-fix-closure-borrows`, `l3-fix-closure-returns-its-argument`, `l4-closures-generic-vs-dyn` | - | L6 is unwritten in anneal. |
| Pattern matching | Ref: patterns; L7 | vending-machine (PAT 1), expr-eval-3vl (QUERY 1), csv (PARSE 3) | `l2-destructure-self`, `y9-shell-line-parser` | - | Binding modes, guards and slice patterns have no dedicated drill (L7 unwritten); they appear inside larger exercises. |
| Const evaluation | Ref: constant evaluation; std (const fn) | alg-bit-flags (BIT 1), checksums (BIN 12) | `l5-const-fn-crc-table` | - | No interview-sized exercise beyond tables and const constructors. |
| Modules, visibility, API design | Ref: crates and source files, names; L9; API guidelines | library-api (TRAIT 8), typestate-conn (TRAIT 7), sbx-capabilities (SBX 1) | `l5-invalid-states-unrepresentable`, `l5-typestate-http-builder` | - | One-file submissions: module trees work inline only. |
| Attributes, conditional compilation | Ref: attributes, conditional compilation | nostd-ring (UNSAFE 2) | - | - | Features and cfg cannot be graded: one fixed build. Learn from the cb-features-workspace crate. |
| Declarative macros | Ref: macros by example; L10 | macro-hashmap (TRAIT 6), derive-builder (TRAIT 10) | `s1-my-result-and-try` | `l10-fix-macro-hygiene` |  |
| Procedural macros | Ref: procedural macros | derive-builder (TRAIT 10) | - | - | Not gradable: needs a proc-macro crate, multi-file builds, and syn and quote (not vendored). |
| Unsafe, raw pointers, MaybeUninit, PhantomData | Ref: unsafety, behaviour considered undefined; std ptr, mem | fixed-vec (UNSAFE 4), myvec (UNSAFE 5), intrusive-list (UNSAFE 6), pin-mapfuture (UNSAFE 7) | `y2-mini-vec`, `y2-array-vec`, `y2-unsafe-linked-list`, `s3-minivec-core`, `l5-phantom-units-of-measure` | - | Tests check behaviour only. Undefined behaviour needs Miri (nightly, not in the runner). |
| FFI and ABI | Ref: external blocks, ABI, linkage; std ffi | ffi-wrapper (UNSAFE 3) | `y3-opaque-handle-api`, `y3-qsort-callback`, `y3-cstring-boundary`, `y3-repr-c-header` | - | Building a cdylib or compiling C is not gradable (one lib.rs, no build script). |
| Inline assembly, runtime, linkage | Ref: inline assembly, the Rust runtime | none | - | - | Left out on purpose: rare in interviews, architecture-specific, no cookbook exercise. |
| Memory model, allocators, layout | Ref: memory model, memory allocation; std alloc | slab (MEM 1), bump-arena (MEM 2), free-list-alloc (MEM 3), buddy (MEM 4), mark-sweep (MEM 5) | `f3-typed-arena`, `f3-buddy-allocator`, `f3-generational-slab`, `s3-predict-reallocations` | - |  |
| Panics and unwinding | Ref: panic; std panic | panic-boundary (ERR 3), thread-pool (CONC 9) | `l1-rollback-guard`, `c2-thread-pool` | - | panic=abort cannot be tested (the test harness needs unwinding). |
| no_std | cookbook cb-no-std | nostd-ring (UNSAFE 2) | - | - | The grader cannot prove the submission is no_std. |
| Cargo, features, workspaces, build scripts | CURRICULUM L9; cookbook crates | library-api (TRAIT 8) | - | - | Not gradable by this runner. Only reading (cb-features-workspace, cb-build-script) or the Courses engine. |

### Standard library (std module index)

| Topic | Outline | Exercises (series part) | Existing | Staged drafts | Gap or reason |
|---|---|---|---|---|---|
| option, result and ? | std option, result; S1 | error-layers (ERR 2) | `s1-my-option`, `s1-transpose`, `s1-collect-into-result`, `s1-my-result-and-try` | - |  |
| Errors: design, context, sources | std error; L8 | error-layers (ERR 2), diagnostics-renderer (ERR 1), axum-api (WEB 3) | `l3-box-dyn-error-downcast`, `s1-fix-question-mark-from` | `l8-import-errors`, `l8-fix-lost-context` |  |
| Strings, char, Unicode, formatting | std str, string, char, fmt, ascii; S2 | utf8-codec (TEXT 1), string-builder (TEXT 2), text-justify (TEXT 4), template-engine (TEXT 5) | `s2-unicode-safe-truncate`, `s2-zero-copy-split-iterator`, `s2-template-formatter`, `s2-display-vs-debug` | - | Grapheme clusters and normalisation need crates that are not vendored. |
| Vec, slices, arrays, sorting, searching | std vec, slice, array; S3 | ring-deque (COLL 2), alg-co-bsearch-greedy (COMBO 1) | `s3-ring-buffer`, `s3-keep-sorted`, `s3-split-at-mut`, `s3-chunks-and-windows` | - |  |
| Maps, sets, ordered collections | std collections, hash; S4 | hashmap-chaining (COLL 3), interval-booking (COLL 10), versioned-map (TXN 1), leaderboard (COLL 11) | `s4-interval-map`, `s4-btreemap-ranges`, `s4-fix-hash-eq-consistency`, `s4-open-addressing-map` | - |  |
| Queues and heaps | std collections; S5 | binary-heap (COLL 4), indexed-pq (COLL 8), minstack-queue (COLL 1), median-stream (COLL 5), ring-deque (COLL 2) | - | `byo:s5-binary-heap`, `byo:s5-indexed-priority-queue` |  |
| Iterators and custom adapters | std iter; S6 | iter-adapters (ITER 1), peekmore-chunks (ITER 4), flatten-tree-iter (ITER 5) | `l4-iterator-extension`, `s3-pairs-iterator`, `l2-stride-iter-mut` | `s6-merge-sorted-streams` |  |
| Smart pointers and interior mutability | std boxed, rc, sync, cell, borrow; S7 | my-refcell (SMART 2), my-rc (SMART 3), arc (ATOM 4), raii-pool (SMART 1), once (ATOM 2) | `c3-mini-arc`, `c3-once-cell`, `l2-fix-borrowmuterror`, `s2-cow-str` | `s7-fix-observer-leak` |  |
| Drop and RAII | Ref: destructors; std ops | raii-pool (SMART 1), semaphore (CONC 5), sbx-quota (SBX 3), mon-spans (MON 4) | `l1-raii-span-guard`, `l1-rollback-guard`, `l1-predict-drop-order` | - |  |
| Comparison, hashing, operator traits | std cmp, ops, marker, default, clone, any; S8 | generic-matrix (TRAIT 3), semver (PARSE 5), alg-bit-flags (BIT 1), event-bus-any (TRAIT 5) | `l4-vec2-operators`, `l4-matrix-operators`, `l4-any-downcasting`, `s1-order-total` | - |  |
| Numbers and floating point | std num, f64; integer types | bigint (NUM 1), units-newtypes (TRAIT 2), knn (SEARCH 9), raster (NUM 2) | `f7-itoa-stack-buffer` | - | Float robustness only has the new preparation part from the ray tracer. |
| Pin | std pin | pin-mapfuture (UNSAFE 7), future-combinators (ASYNC 16) | `c5-block-on` | - | Unsound pinning cannot be detected by tests. |
| I/O traits and buffering | std io; S9 | bounded-line-reader (IO 2), bufreader (IO 3), io-adapters (IO 4), log-analyzer (IO 1) | `s2-word-frequency` | `s9-bounded-line-reader` |  |
| Files and paths | std fs, path; S9 | atomic-file (IO 5), dir-walker (IO 6), sec-safe-path (SEC 1), positional-block-io (IO 8) | - | - | Writable temp files in the Docker sandbox are not verified; most exercises use a trait over an in-memory file system. |
| Processes, env, signals | std process, env, os; S10 | process-pipeline (IO 7), cli-args (PARSE 7), sbx-command-policy (SBX 2), sbx-supervisor (SBX 6) | `y9-shell-line-parser` | - | Spawning real processes is unverified; signals need libc, not vendored. |
| Networking | std net; B4 | tcp-echo-framed (NET 5), tcp-reassembly (NET 2), reliable-udp (NET 3), sans-io (NET 1), tokio-tcp-server (NET 13) | `b4-tcp-reassembler`, `b4-length-prefixed-codec`, `b4-http1-request-parser` | - | Loopback inside the --network none sandbox is not verified; DNS and external hosts are impossible. |
| Time | std time; S10 | fake-clock (TEST 1), datetime-parse (PARSE 9), job-scheduler (CONC 15) | - | - | Real-time behaviour is never asserted. |
| Threads and thread locals | std thread; C1 | thread-pool (CONC 9), parallel-mapreduce (CONC 13), work-stealing (CONC 19), ambient-context (SMART 4) | `c2-thread-pool`, `l1-move-into-threads` | - |  |
| Locks, Condvar, Barrier | std sync; C1 | blocking-queue (CONC 2), semaphore (CONC 5), rwlock (CONC 17), barrier-latch (CONC 6), bank-transfer-concurrent (CONC 3) | `c1-blocking-queue`, `c1-fair-semaphore`, `c1-rwlock-writer-preferring` | `c1-fix-transfer-deadlock` |  |
| Channels | std sync::mpsc; C2 | channel-mpsc (CONC 7), oneshot-thread (CONC 8), pubsub (CONC 12), pipeline-stages (CONC 14), actor (CONC 16) | `c2-bounded-channel` | - | select over std channels needs crossbeam. |
| Send and Sync | Ref: special types and traits | send-across-await (ASYNC 4), manual-vtable (TRAIT 9), spsc (ATOM 5) | `l4-fix-dyn-send`, `l3-fix-thread-spawn-static` | - | A wrong unsafe impl Send passes behaviour tests; needs review or Miri. |
| Atomics and memory ordering | std sync::atomic; Ref: memory model; C3 | spinlock (ATOM 1), once (ATOM 2), spsc (ATOM 5), mpmc (ATOM 7), treiber (ATOM 8), seqlock (ATOM 6), atomic-counters (MON 1), interleaving-checker (ATOM 9) | `c3-spinlock`, `c3-spsc-ring`, `c3-bounded-mpmc`, `c3-index-stack` | - | Ordering bugs mostly pass on x86 and often on ARM; loom is not vendored and Miri is unavailable. |

### Async (std future, task; C4, C5)

| Topic | Outline | Exercises (series part) | Existing | Staged drafts | Gap or reason |
|---|---|---|---|---|---|
| Futures, wakers, executors | std future, task; C5 | block-on (ASYNC 1), executor (ASYNC 2), async-channel (ASYNC 3), future-combinators (ASYNC 16), task-wake-state (ASYNC 15) | `c5-block-on`, `c5-mini-executor`, `c5-oneshot-channel` | - |  |
| Cancellation safety and select | tokio; C4 | cancel-safety (ASYNC 6), async-primitives (ASYNC 10), graceful-shutdown (ASYNC 7) | - | `c4-fix-cancel-safe-reader`, `c4-drain-with-deadline` |  |
| Backpressure, bounded concurrency, streams | C4 | fanout-limit (ASYNC 5), stream-adapters (ASYNC 8), async-batcher (ASYNC 9), middleware-tower (WEB 4) | - | `c4-bounded-fan-out`, `b2-fix-poll-ready` |  |

### Engineering practice

| Topic | Outline | Exercises (series part) | Existing | Staged drafts | Gap or reason |
|---|---|---|---|---|---|
| Testing: unit, model-based, property | CURRICULUM Y5; cookbook testing | write-tests (TEST 2), shrinker (TEST 3), proptest-use (TEST 4), test-differential (TEST 6) | - | `y5-catch-the-broken-queue`, `y5-shrink-failing-input` | Grading a learner's tests needs the mutant pattern of the staged queue problem. |
| Fuzzing, loom, Miri, compile-fail tests | cookbook testing | fuzz-harness (TEST 5), interleaving-checker (ATOM 9) | - | - | cargo-fuzz and Miri need nightly; loom and trybuild are not vendored; doctests are off. |
| Simulation and fault injection | cookbook distributed | sim-network (DIST 1), test-failpoints (TEST 7), disk-fault-model (TEST 10), sim-invariant-search (TEST 12) | - | - |  |
| Performance and profiling | CURRICULUM F1, F2, F5 | alloc-budget (PERF 2), cache-friendly (PERF 3), bench-harness (PERF 1), simd-scan (PERF 4), mon-flame (MON 7) | `f2-particles-as-arrays`, `f5-swar-memchr`, `f5-bitset-iter` | - | criterion and flamegraphs cannot grade; only [perf] helpers, and timing can flake. |
| Serialization and encodings | CURRICULUM F7 | varint-codec (BIN 1), serde-wire (BIN 3), serde-format (BIN 11), protobuf-wire (BIN 5), base64 (TEXT 3), checksums (BIN 12) | `f5-varint-codec`, `f7-posting-list-codec` | `f7-wire-format-json` |  |
| Observability | CURRICULUM B6 | mon-metrics (MON 2), mon-spans (MON 4), mon-slo (MON 5), mon-flight-recorder (MON 3) | - | - | tracing is vendored; subscribers are testable in process only. |

### Interview subject areas (cookbook outline)

| Topic | Outline | Exercises (series part) | Existing | Staged drafts | Gap or reason |
|---|---|---|---|---|---|
| Data-structure design and machine coding | CURRICULUM M2; cookbook interview | rate-limiter (RESIL 2), order-book (PAT 14), bank-ledger (PAT 6), parking-lot (PAT 5), ttl-kv (CACHE 2), pubsub (CONC 12), elevator (PAT 12), ticket-booking (PAT 13) | `f4-open-addressing-map` | `byo:m2-rate-limiter`, `f4-lru-cache-arena` |  |
| Parsers and languages | cookbook compilers, text_and_parsing | tokenizer-borrowed (PARSE 2), pratt (PARSE 12), json (PARSE 13), regex-nfa (PARSE 15), tree-interpreter (LANG 4), bytecode-vm (LANG 5) | `y8-regex-nfa-matcher`, `y6-wasm-stack-interpreter` | `y14-pratt-parser` |  |
| Databases | cookbook database | wal (STORE 2), slotted-page (STORE 3), buffer-pool (STORE 9), disk-bptree (STORE 13), volcano (QUERY 2), mvcc-si (TXN 3), lock-manager (TXN 4) | `f2-slotted-page`, `y9-kv-store-wal` | `y12-mvcc-snapshot-isolation` | SQL against a real database needs a database crate and server. |
| Distributed systems and CRDTs | cookbook distributed | sim-network (DIST 1), raft-election (DIST 18), quorum-kv (DIST 10), crdt-counters (CRDT 1), crdt-map (CRDT 7), two-phase-commit (DIST 15) | `y7-raft-vote-rules`, `y9-ot-collaborative-text` | `y13-or-set-crdt` |  |
| Web and security | cookbook web, cryptography; B1, B2, B5 | router (WEB 1), axum-api (WEB 3), jwt-hmac (SEC 4), sec-safe-path (SEC 1), authz (SEC 6) | `b4-http1-request-parser` | `b1-api-error-response`, `b5-fix-webhook-verify` | reqwest, hyper client, databases are not vendored. |
| Operating systems and sandboxing | cookbook os_and_storage | cpu-scheduler (OS 1), page-table (OS 4), fs-blocks (OS 5), sbx-fuel (SBX 5), sbx-supervisor (SBX 6) | - | - | Namespaces, seccomp and containers: no cookbook recipe and not gradable. |
| Search and vectors | cookbook search_vectors | inverted-index (SEARCH 1), bm25 (SEARCH 5), autocomplete (SEARCH 6), knn (SEARCH 9), hnsw (SEARCH 14) | `y10-trigram-index`, `y10-bm25-ranking` | - |  |
| Algorithm techniques | content/dsa patterns; cookbook algorithms | alg-recursion-fs (REC 1), alg-order (GRAPH 4), alg-dp-knapsack (DP 3), alg-iv-merge (IVL 1), alg-gr-select (GRD 1), alg-bit-flags (BIT 1), alg-co-dag-dp (COMBO 4) | `p1-cable-network` | - | The DSA area tracks LeetCode problems; these are Rust, scenario-based and complementary. |
| Data lineage and event history | owner request | lin-event-store (LIN 4), lin-graph (LIN 7), lin-column (LIN 9), lin-recompute (LIN 8) | - | - | No cookbook page is about lineage. |
| Maths and graphics | cookbook math_science, graphics, hpc | bigint (NUM 1), raster (NUM 2), ray-intersection (NUM 3), monte-carlo (NUM 8) | - | - | GPU, MPI and real HPC clusters are out of reach. |

## 2. Counts and how thorough

Counted by the script that built this file.

| Measure | Count |
|---|---|
| Short exercises (parts) | 435 |
| of which preparation parts added from the capstone readings | 62 |
| Series | 42 |
| Parts in 45 / 60 / 90 / 120 minute boxes | 74 / 190 / 108 / 63 |
| Estimated hours, all parts | 534 |
| Already covered by an existing problem (full) | 37 |
| Covered by a staged draft (including the six) | 27 |
| To be written | 371 |
| Main-path parts / branch parts | 150 / 285 |
| Parts with 0 / 1 / 2 direct requirements | 80 / 325 / 30 |
| Links hidden until asked | 47 |
| Capstones (16 cookbook, 3 multi-file, rest composed) | 32 |
| Capstones with milestones and readiness from a full read | 16 |
| Milestones in those / readiness rows | 239 / 318 |
| Concept-map concepts | 53 |
| Coverage-matrix rows | 65 |
| Existing problem folders whose ids were checked (all areas) | 282 |
| Cookbook pages whose ids were checked | 704 |
| Cookbook links kept 'exact' / downgraded to 'similar*' | 24 / 182 |

By kind of series: core 385 parts, branch 9 parts, algorithm 31 parts, combination 10 parts.

**How thorough, honestly.**

- **Read in full:** the 16 capstone source files, first line to last (readers' line counts are in section 7). That reading was split among six parallel readers; I checked their data by machine (ids, minutes, milestone references), not by rereading the files. Their bug notes come from reading only.
- **Code read for the exact links:** the parts of the files named under each exact link that solve the core.
- **The other Cookbook example files (463 mapped to pages):** I read a generated digest of every one: its title line, level, section banners and type or function names. That is reading structure, not bodies. For the interview folder I also read the question index (`QUESTIONS.md`) from question 86 on and the first entries before it.
- **Multi-file Cookbook crates** (`cb-compiler`, `cb-compiler-tools`, `cb-backend-reference` and the small ones): README openings and file lists only. **Their capstone entries and every composed capstone have outline stages and a provisional readiness list, not one built from reading code.**
- `cookbook/projects/*.md` (about 80 project briefs): headings only. `cookbook/practice/*.md`: sampled.
- **Existing anneal problems:** I read the opening 110 to 300 characters of the 248 Rust statements and matched by hand. "full", "partly" and "similar" coverage is that judgement. The DSA area was read only for its pattern and technique names.
- **Staged drafts:** the 20 in `anneal-staging/rust-practice` by title, sidecar and one problem in full; the six new ones as written.
- Interview-sized scope: where a topic does not fit 120 minutes it was cut (the entry says so) or left to a capstone.

## 3. Series overview

A series ends in a capstone. Where the capstone uses only some of a series, the table says how many parts its readiness list actually names; for the algorithm and a few core series that link is loose, and I say so rather than stretch it.

| Key | Series | Kind | Parts | Hours (est.) | Written or covered | Ends in | Parts the capstone's readiness names | Take alongside |
|---|---|---|---|---|---|---|---|---|
| OWN | Ownership and API design | core | 8 | 8.8 | 0 | `cap-lox` | 0 | `iv-fake-clock` |
| ITER | Closures and iterators | core | 5 | 5.5 | 1 | `cap-sql` | 0 | `iv-write-tests` |
| TRAIT | Traits, generics and public APIs | core | 10 | 11.2 | 2 | `cap-httpserver` | 0 | - |
| ERR | Errors and panic safety | core | 3 | 2.8 | 1 | `cap-jobsvc` | 1 | - |
| TEXT | Strings, Unicode and text | core | 6 | 5.8 | 0 | `cap-typeahead` | 1 | `iv-fuzz-harness` |
| SMART | Smart pointers, interior mutability and RAII | core | 4 | 4.8 | 1 | `cap-runtime` | 1 | - |
| PAT | Modelling with Rust (machine coding) | core | 15 | 17.2 | 0 | `cap-ledger` | 2 | `iv-fake-clock`, `iv-mon-metrics` |
| UNSAFE | Unsafe Rust and FFI | core | 7 | 9.2 | 3 | `cap-unsafekit` | 3 (provisional) | `iv-interleaving-checker` |
| MEM | Allocators and memory management | core | 5 | 7.8 | 2 | `cap-interp` | 1 (provisional) | `iv-alloc-budget` |
| COLL | Collections from scratch | core | 18 | 25.5 | 5 | `cap-redis` | 3 | `iv-write-tests`, `iv-shrinker` |
| CACHE | Caches and bounded memory | core | 10 | 10.2 | 3 | `cap-redis` | 2 | `iv-mon-metrics` |
| CONC | Concurrency: locks to a job system | core | 22 | 24.5 | 6 | `cap-jobsched` | 1 (provisional) | `iv-mon-lockdep`, `iv-mon-metrics` |
| ATOM | Atomics and lock-free structures | core | 9 | 12.0 | 5 | `cap-unsafekit` | 2 (provisional) | `iv-interleaving-checker` |
| ASYNC | Async: from a Future to a small runtime | core | 17 | 21.2 | 5 | `cap-runtime` | 8 | `iv-mon-spans`, `iv-fake-clock` |
| PARSE | Parsers: from tokens to full formats | core | 22 | 26.2 | 4 | `cap-sql` | 2 | `iv-fuzz-harness`, `iv-shrinker` |
| LANG | From parser to a language | core | 16 | 25.8 | 1 | `cap-lox` | 8 | `iv-test-differential` |
| BIN | Binary formats and wire protocols | core | 13 | 15.2 | 2 | `cap-queue` | 2 (provisional) | `iv-fuzz-harness` |
| IO | Files, processes and standard I/O | core | 8 | 7.2 | 1 | `cap-fs` | 1 | `iv-test-failpoints` |
| OS | Operating-system ideas | core | 8 | 10.5 | 0 | `cap-fs` | 3 | `iv-mon-metrics` |
| NET | Networking: bytes to a server | core | 13 | 19.5 | 2 | `cap-httpserver` | 6 | `iv-mon-spans` |
| WEB | Web services | core | 9 | 10.0 | 2 | `cap-jobsvc` | 4 | `iv-mon-metrics`, `iv-mon-spans` |
| SEC | Security | core | 8 | 7.8 | 1 | `cap-backend` | 2 (provisional) | `iv-fuzz-harness` |
| SBX | Task sandboxing | core | 6 | 7.2 | 0 | `cap-jobsched` | 2 (provisional) | `iv-mon-flight-recorder` |
| STORE | Storage engine | core | 20 | 28.5 | 3 | `cap-kvstore` | 5 | `iv-test-failpoints`, `iv-mon-metrics` |
| QUERY | Query engine | core | 15 | 20.0 | 0 | `cap-optimizer` | 9 | `iv-test-differential`, `iv-mon-flame` |
| TXN | Transactions | core | 6 | 8.0 | 2 | `cap-database` | 1 (provisional) | `iv-interleaving-checker`, `iv-test-differential` |
| SEARCH | Search and similarity | core | 17 | 19.8 | 3 | `cap-search` | 7 | `iv-mon-metrics` |
| RESIL | Resilience patterns | core | 12 | 12.8 | 1 | `cap-ratelimitlab` | 4 | `iv-mon-slo`, `iv-mon-metrics` |
| DIST | Distributed systems to consensus | core | 23 | 33.2 | 2 | `cap-raftkv` | 8 (provisional) | `iv-sim-network`, `iv-mon-spans`, `iv-linearizability` |
| CRDT | CRDTs: replicas that converge | core | 15 | 22.8 | 2 | `cap-collab` | 6 (provisional) | `iv-crdt-convergence`, `iv-sim-network` |
| LIN | Data lineage and event history | core | 10 | 12.2 | 1 | `cap-lineage` | 5 (provisional) | `iv-test-differential` |
| MON | Monitoring and debugging | core | 9 | 8.5 | 1 | `cap-loadbalancer` | 1 (provisional) | - |
| TEST | Testing and reliability | core | 12 | 12.0 | 2 | `cap-lease` | 1 | - |
| PERF | Performance | core | 4 | 4.5 | 0 | `cap-raytracer` | 1 | `iv-mon-flame` |
| NUM | Numbers and pixels | branch | 9 | 9.8 | 0 | `cap-raytracer` | 7 | `iv-test-differential` |
| REC | Algorithms: recursion to memoisation | algorithm | 6 | 6.5 | 0 | `cap-lox` | 0 | `iv-write-tests` |
| GRAPH | Algorithms: graphs in Rust | algorithm | 7 | 7.8 | 0 | `cap-lineage` | 0 (provisional) | `iv-test-differential` |
| DP | Algorithms: dynamic programming | algorithm | 7 | 7.0 | 0 | `cap-optimizer` | 0 | `iv-test-differential` |
| IVL | Algorithms: intervals | algorithm | 4 | 4.5 | 0 | `cap-tsdb` | 0 (provisional) | `iv-test-differential` |
| GRD | Algorithms: greedy | algorithm | 3 | 2.5 | 0 | `cap-jobsched` | 0 (provisional) | `iv-test-differential` |
| BIT | Algorithms: bit manipulation | algorithm | 4 | 3.5 | 0 | `cap-fs` | 0 | `iv-write-tests` |
| COMBO | Combinations: using several techniques at once | combination | 10 | 13.8 | 0 | `cap-optimizer` | 0 | `iv-test-differential` |

**Loose endings** (the assigned capstone's readiness names no part of the series): OWN, ITER, TRAIT, REC, GRAPH, DP, IVL, GRD, BIT, COMBO.

## 4. Roadmap

One ordered main path of 10 stages. Do the parts of a stage in the order listed. When a stage is done, you know what is next: the following stage. Branches are optional. A script checked that the required links have no cycle and that every required part comes earlier in this order (main path, then the branch order).

### Stage 1: Rust working set (13 parts, about 12 h)

- **OWN** 1. split-borrow-bank, 2. linked-list-safe, 3. view-splitter
- **ITER** 1. iter-adapters, 2. fn-pipeline, 3. memoize
- **ERR** 1. diagnostics-renderer, 2. error-layers
- **TRAIT** 1. shape-registry, 2. units-newtypes
- **SMART** 1. raii-pool
- **TEST** 1. fake-clock, 2. write-tests

### Stage 2: First structures (16 parts, about 14 h)

- **COLL** 1. minstack-queue, 2. ring-deque, 3. hashmap-chaining, 4. binary-heap
- **REC** 1. alg-recursion-fs, 2. alg-ast-walk, 3. alg-config-matrix
- **BIT** 1. alg-bit-flags, 2. alg-bit-fields
- **PARSE** 1. rpn-calc, 2. tokenizer-borrowed, 3. csv
- **PAT** 1. vending-machine, 2. url-shortener, 3. snake, 4. board-game

### Stage 3: Structures that carry later work (21 parts, about 20 h)

- **OWN** 4. undo-stack, 5. text-gap-buffer, 6. graph-ownership
- **CACHE** 1. lru, 2. ttl-kv, 3. hit-counter, 4. bloom
- **COLL** 5. median-stream, 6. trie, 7. union-find, 8. indexed-pq, 9. bitset-roaring, 10. interval-booking, 11. leaderboard
- **TEST** 3. shrinker
- **GRAPH** 1. alg-graph-repr, 2. alg-reach, 3. alg-regions, 4. alg-order
- **IVL** 1. alg-iv-merge, 2. alg-iv-sweep

### Stage 4: Threads (16 parts, about 14 h)

- **CONC** 1. print-in-order, 2. blocking-queue, 3. bank-transfer-concurrent, 4. dining, 5. semaphore, 6. barrier-latch, 7. channel-mpsc, 8. oneshot-thread, 9. thread-pool
- **SMART** 2. my-refcell
- **ATOM** 1. spinlock, 2. once, 3. rcu-snapshot
- **ERR** 3. panic-boundary
- **MON** 1. atomic-counters, 2. mon-metrics

### Stage 5: Bytes and files (19 parts, about 18 h)

- **BIN** 1. varint-codec, 2. length-codec
- **IO** 1. log-analyzer, 2. bounded-line-reader, 3. bufreader
- **STORE** 1. row-serialization, 2. wal, 3. slotted-page
- **PARSE** 4. ini-toml, 5. semver, 6. url-parser, 7. cli-args, 8. shell-line, 9. datetime-parse, 10. glob, 11. recursive-descent, 12. pratt
- **TEST** 4. proptest-use, 5. fuzz-harness

### Stage 6: Storage to queries (18 parts, about 22 h)

- **STORE** 4. heap-file, 5. memtable-flush, 6. compaction, 7. bloom-sstable
- **QUERY** 1. expr-eval-3vl, 2. volcano, 3. external-sort
- **TXN** 1. versioned-map, 2. serializability-checker
- **REC** 4. alg-rota, 5. alg-regex-count
- **DP** 1. alg-dp-1d, 2. alg-dp-grid, 3. alg-dp-knapsack, 4. alg-dp-strings
- **PARSE** 13. json, 14. markdown, 15. regex-nfa

### Stage 7: Async and scheduling (13 parts, about 14 h)

- **CONC** 10. sharded-map, 11. singleflight, 12. pubsub, 13. parallel-mapreduce, 14. pipeline-stages, 15. job-scheduler
- **ASYNC** 1. block-on, 2. executor, 3. async-channel, 4. send-across-await, 5. fanout-limit, 6. cancel-safety, 7. graceful-shutdown

### Stage 8: Services (12 parts, about 12 h)

- **RESIL** 1. retry-backoff, 2. rate-limiter, 3. circuit-breaker, 4. idempotency-store
- **WEB** 1. router, 2. cursor-pagination
- **NET** 1. sans-io
- **SEC** 1. sec-safe-path, 2. sec-injection, 3. sec-ct-compare
- **MON** 3. mon-flight-recorder, 4. mon-spans

### Stage 9: Distributed (13 parts, about 14 h)

- **DIST** 1. sim-network, 2. dist-delivery, 3. visibility-queue, 4. dist-heartbeat, 5. gossip-failure, 6. lamport-vclock, 7. lease-fencing, 8. consistent-hash
- **CRDT** 1. crdt-counters, 2. crdt-registers, 3. crdt-set
- **TEST** 6. test-differential, 7. test-failpoints

### Stage 10: Consensus and convergence (9 parts, about 14 h)

- **DIST** 9. dist-sharding, 10. quorum-kv, 11. dist-anti-entropy, 12. partitioned-log, 13. stream-windows, 14. cdc
- **CRDT** 4. crdt-convergence, 5. crdt-causal-context, 6. crdt-orswot

### Optional branches

Each row is the rest of a series. **Opens after** is the earliest main-path stage by which every requirement from outside the branch is on the main path; a later stage is fine. `also needs` lists requirements that sit in other branches.

| Branch | Parts | Opens after stage | Also needs |
|---|---|---|---|
| OWN 7-8 | 2 | 3 |  |
| ITER 4-5 | 2 | 1 |  |
| TRAIT 3-10 | 8 | 1 |  |
| TEXT 1-6 | 6 | any |  |
| SMART 3-4 | 2 | 4 |  |
| PAT 5-15 | 11 | 8 |  |
| UNSAFE 1-7 | 7 | 1 |  |
| MEM 1-5 | 5 | 3 | `iv-fixed-vec` (UNSAFE 4) |
| COLL 12-18 | 7 | 3 | `iv-ray-intersection` (NUM 3), `iv-slab` (MEM 1) |
| CACHE 5-10 | 6 | 4 |  |
| CONC 16-22 | 7 | 7 |  |
| ATOM 4-9 | 6 | 4 | `iv-my-rc` (SMART 3) |
| ASYNC 8-17 | 10 | 8 | `iv-actor` (CONC 16) |
| PARSE 16-22 | 7 | 6 |  |
| LANG 1-16 | 16 | 5 | `iv-parser-combinators` (PARSE 16) |
| BIN 3-13 | 11 | 5 | `iv-layout-quiz-codec` (UNSAFE 1) |
| IO 4-8 | 5 | 5 | `iv-flatten-tree-iter` (ITER 5) |
| OS 1-8 | 8 | 5 | `iv-file-system-tree` (PAT 11), `iv-spsc` (ATOM 5) |
| NET 2-13 | 12 | 9 |  |
| WEB 3-9 | 7 | 8 |  |
| SEC 4-8 | 5 | 8 |  |
| SBX 1-6 | 6 | 8 | `iv-bytecode-vm` (LANG 5), `iv-typestate-conn` (TRAIT 7) |
| STORE 8-20 | 13 | 6 | `iv-actor` (CONC 16), `iv-atomic-file` (IO 5), `iv-bptree-memory` (COLL 16), `iv-checksums` (BIN 12), `iv-disk-fault-model` (TEST 10) |
| QUERY 4-15 | 12 | 6 | `iv-alg-co-bitmask-dp` (COMBO 6), `iv-reservoir` (CACHE 7) + 1 hidden |
| TXN 3-6 | 4 | 6 |  |
| SEARCH 1-17 | 17 | 6 |  |
| RESIL 5-12 | 8 | 8 |  |
| DIST 15-23 | 9 | 10 |  |
| CRDT 7-15 | 9 | 10 |  |
| LIN 1-10 | 10 | 6 |  |
| MON 5-9 | 5 | 8 | `iv-alg-bit-tricks` (BIT 3) |
| TEST 8-12 | 5 | 9 | `iv-positional-block-io` (IO 8) |
| PERF 1-4 | 4 | 1 | `iv-alg-bit-tricks` (BIT 3) |
| NUM 1-9 | 9 | any | `iv-generic-matrix` (TRAIT 3), `iv-seeded-rng` (TEST 8) |
| REC 6-6 | 1 | 6 |  |
| GRAPH 5-7 | 3 | 3 |  |
| DP 5-7 | 3 | 6 |  |
| IVL 3-4 | 2 | 3 |  |
| GRD 1-3 | 3 | 3 |  |
| BIT 3-4 | 2 | 2 |  |
| COMBO 1-10 | 10 | 6 |  + 3 hidden |

<details><summary>The full branch order used for the cycle and order check</summary>

scoped-callbacks, lending-windows, peekmore-chunks, flatten-tree-iter, generic-matrix, heterogeneous-store, event-bus-any, macro-hashmap, typestate-conn, library-api, manual-vtable, derive-builder, utf8-codec, string-builder, base64, text-justify, template-engine, unicode-search-keys, my-rc, ambient-context, parking-lot, bank-ledger, splitwise, feature-flags, logger, notification, file-system-tree, elevator, ticket-booking, order-book, spreadsheet, layout-quiz-codec, nostd-ring, ffi-wrapper, fixed-vec, myvec, intrusive-list, pin-mapfuture, slab, bump-arena, free-list-alloc, buddy, mark-sweep, range-queries, cuckoo, ordered-map-skiplist, treap-order-stats, bptree-memory, count-min, hll, reservoir, lfu, generation-cache, sampled-expiry, actor, rwlock, timing-wheel, work-stealing, chunk-queue, parked-waiters, park-wake-handshake, arc, spsc, seqlock, mpmc, treiber, interleaving-checker, stream-adapters, async-batcher, async-primitives, async-trait-object, async-retry-hedge, tokio-job-outcome, async-sync-bridge, task-wake-state, future-combinators, register-recheck, parser-combinators, json-streaming, error-recovery, stmt-parser-desugar, regex-syntax, nfa-anchors-search, cps-backtracker, sexpr-lisp, constant-fold, typecheck-simple, tree-interpreter, bytecode-vm, dataflow, peg-packrat, formatter, hindley-milner, unification, wasm-interp, ssa-regalloc, interp-call-frames, interp-object-model, vm-local-slots, vm-upvalues, serde-wire, resp, protobuf-wire, packet-parse, gorilla, lz77, huffman, dns-codec, serde-format, checksums, png-container, io-adapters, atomic-file, dir-walker, process-pipeline, positional-block-io, cpu-scheduler, page-replacement, disk-sched-raid, page-table, fs-blocks, io-uring-model, inode-block-map, unix-namespace, tcp-reassembly, reliable-udp, websocket-frames, tcp-echo-framed, http-request, http-server, event-loop, tokio-tcp-conn, mio-reactor-api, http-response-framing, tcp-lingering-close, tokio-tcp-server, axum-api, middleware-tower, http-cache, webhook-sender, sse-replay, axum-request-tracing, http-range, jwt-hmac, sessions, authz, sec-egress, sec-passwords, sbx-capabilities, sbx-command-policy, sbx-quota, sbx-vfs-jail, sbx-fuel, sbx-supervisor, lru-k, buffer-pool, columnar-encodings, bitmap-index, extendible-hash, disk-bptree, checkpoint-recovery, latch-crabbing, hash-agg, hash-join, merge-join, sql-parser, mini-sql-exec, planner-rules, window-functions, asof-join, cardinality, mvcc-si, lock-manager, occ, timestamp-ordering, inverted-index, posting-compression, boolean-query, trigram, bm25, autocomplete, fuzzy, aho-corasick, knn, lsh-dedupe, spatial-grid, suffix-array, ivf, hnsw, qgram-typo-filter, positional-phrase, immutable-segments, load-balancer, service-registry, outbox, saga, snowflake, window-limiters, gcra-equivalence, aimd-concurrency, two-phase-commit, linearizability, raft-core, raft-election, raft-replication, raft-apply, raft-persist, raft-snapshot, raft-reads, crdt-map, crdt-delta, crdt-opbased, crdt-gc, text-crdt, crdt-text-fugue, crdt-undo, crdt-movable-tree, ot, diff, merkle, git-objects, lin-event-store, lin-projections, lin-upcasting, lin-graph, lin-recompute, lin-column, decide-evolve, mon-slo, mon-lockdep, mon-flame, mon-anomaly, seeded-rng, grammar-program-gen, disk-fault-model, redo-journal, effect-log, group-commit, online-log-rewrite, crash-ordered-files, seeded-data-gen, sim-invariant-search, bench-harness, alloc-budget, cache-friendly, bigint, raster, ray-intersection, bvh-flat, skiplist-spans, float-robust, camera-rays, direct-shading, surface-optics, monte-carlo, mesh-subdivision, alg-memo-resolver, alg-routes, alg-scc, alg-flow, alg-dp-lis, alg-dp-state-machine, alg-dp-interval, alg-iv-set, alg-iv-queries, alg-gr-select, alg-gr-deadlines, alg-gr-huffman, alg-bit-tricks, log-linear-histogram, simd-scan, alg-bit-xor, alg-co-bsearch-greedy, alg-co-weighted-intervals, alg-co-greedy-heap, alg-co-dag-dp, alg-co-graph-heap, alg-co-bitmask-dp, join-order, predicate-selectivity, interesting-orders, alg-co-tree-dp, alg-co-bt-bitset, alg-co-offline-dsu, alg-co-window-deque-dp

</details>

### Concept map

Each concept is first taught once, practised again somewhere different, and applied in a project. **This table names links that are hidden elsewhere**; read it to plan, not just before attempting a part. The order is the roadmap order.

| Concept | First taught | Practised again (roadmap order) | Applied in |
|---|---|---|---|
| Index handles instead of references | graph-ownership (OWN 6) | lru (CACHE 1), trie (COLL 6), alg-graph-repr (GRAPH 1), executor (ASYNC 2), slab (MEM 1), ordered-map-skiplist (COLL 14), bptree-memory (COLL 16) | `cap-database` |
| Linked order kept by indices (recency lists) | linked-list-safe (OWN 2) | lru (CACHE 1), lfu (CACHE 8), lru-k (STORE 8), buffer-pool (STORE 9) | `cap-database` |
| Free list and generations | slab (MEM 1) | free-list-alloc (MEM 3), timing-wheel (CONC 18), fs-blocks (OS 5) | `cap-fs` |
| Ring buffer | ring-deque (COLL 2) | hit-counter (CACHE 3), mon-flight-recorder (MON 3), timing-wheel (CONC 18), spsc (ATOM 5), io-uring-model (OS 6) | `cap-runtime` |
| Heap and priority order | binary-heap (COLL 4) | median-stream (COLL 5), indexed-pq (COLL 8), compaction (STORE 6), external-sort (QUERY 3), job-scheduler (CONC 15), alg-routes (GRAPH 5), alg-co-greedy-heap (COMBO 3) | `cap-jobsched` |
| Updating priority by position | indexed-pq (COLL 8) | order-book (PAT 14), lru-k (STORE 8), hnsw (SEARCH 14) | `cap-jobsched` |
| Merging sorted streams | alg-iv-merge (IVL 1) | compaction (STORE 6), external-sort (QUERY 3), merge-join (QUERY 6) | `cap-kvstore` |
| Disjoint sets | union-find (COLL 7) | hindley-milner (LANG 9), lsh-dedupe (SEARCH 10), alg-co-offline-dsu (COMBO 9) | `cap-interp` |
| Prefix tree | trie (COLL 6) | pubsub (CONC 12), router (WEB 1), file-system-tree (PAT 11), autocomplete (SEARCH 6), aho-corasick (SEARCH 8) | `cap-typeahead` |
| Tree of partial aggregates | leaderboard (COLL 11) | range-queries (COLL 12), treap-order-stats (COLL 15), window-functions (QUERY 10) | `cap-optimizer` |
| Ordered-map neighbour queries | interval-booking (COLL 10) | versioned-map (TXN 1), consistent-hash (DIST 8), tcp-reassembly (NET 2), alg-iv-set (IVL 3) | `cap-kvstore` |
| Sweep over events | alg-iv-sweep (IVL 2) | alg-iv-queries (IVL 4), alg-co-weighted-intervals (COMBO 2), alg-co-greedy-heap (COMBO 3) | `cap-jobsched` |
| Injected clock | fake-clock (TEST 1) | ttl-kv (CACHE 2), job-scheduler (CONC 15), retry-backoff (RESIL 1), rate-limiter (RESIL 2), lease-fencing (DIST 7), mon-slo (MON 5) | `cap-ratelimitlab` |
| Seeded random testing against a model | write-tests (TEST 2) | shrinker (TEST 3), sim-network (DIST 1), test-differential (TEST 6), crdt-convergence (CRDT 4) | `cap-raftkv` |
| Shrinking a failure | shrinker (TEST 3) | test-differential (TEST 6), crdt-convergence (CRDT 4), linearizability (DIST 16) | `cap-lease` |
| Deterministic simulation | sim-network (DIST 1) | dist-delivery (DIST 2), crdt-convergence (CRDT 4), two-phase-commit (DIST 15), raft-election (DIST 18) | `cap-raftkv` |
| Crash at every point | wal (STORE 2) | test-failpoints (TEST 7), checkpoint-recovery (STORE 14), two-phase-commit (DIST 15), raft-persist (DIST 21) | `cap-kvstore` |
| Checksummed append-only log | wal (STORE 2) | memtable-flush (STORE 5), partitioned-log (DIST 12), checkpoint-recovery (STORE 14), raft-replication (DIST 19), lin-event-store (LIN 4) | `cap-kvstore` |
| Incremental parsing: need more bytes | length-codec (BIN 2) | cancel-safety (ASYNC 6), sans-io (NET 1), json-streaming (PARSE 17), resp (BIN 4), websocket-frames (NET 4), http-request (NET 6) | `cap-redis` |
| Recursive descent and precedence | recursive-descent (PARSE 11) | pratt (PARSE 12), json (PARSE 13), regex-nfa (PARSE 15), sql-parser (QUERY 7), boolean-query (SEARCH 3) | `cap-sql` |
| Zero-copy borrowing | view-splitter (OWN 3) | tokenizer-borrowed (PARSE 2), url-parser (PARSE 6), scoped-callbacks (OWN 7) | `cap-lox` |
| Spans and diagnostics | diagnostics-renderer (ERR 1) | template-engine (TEXT 5), error-recovery (PARSE 18), typecheck-simple (LANG 3), sql-parser (QUERY 7) | `cap-lox` |
| State machine as an enum | csv (PARSE 3) | vending-machine (PAT 1), circuit-breaker (RESIL 3), sans-io (NET 1), sbx-supervisor (SBX 6), saga (RESIL 8), raft-core (DIST 17) | `cap-redis` |
| RAII guard | raii-pool (SMART 1) | semaphore (CONC 5), spinlock (ATOM 1), mon-spans (MON 4), sbx-quota (SBX 3), buffer-pool (STORE 9) | `cap-httpserver` |
| Condition variable wait loop | blocking-queue (CONC 2) | semaphore (CONC 5), barrier-latch (CONC 6), oneshot-thread (CONC 8), singleflight (CONC 11), rwlock (CONC 17) | `cap-httpserver` |
| Lock ordering and cycles | bank-transfer-concurrent (CONC 3) | dining (CONC 4), ticket-booking (PAT 13), latch-crabbing (STORE 15), lock-manager (TXN 4), mon-lockdep (MON 6) | `cap-database` |
| Backpressure | blocking-queue (CONC 2) | channel-mpsc (CONC 7), pubsub (CONC 12), pipeline-stages (CONC 14), async-channel (ASYNC 3), http-server (NET 7), middleware-tower (WEB 4) | `cap-queue` |
| Panic isolation | thread-pool (CONC 9) | panic-boundary (ERR 3), ffi-wrapper (UNSAFE 3), actor (CONC 16) | `cap-jobsvc` |
| Acquire and release ordering | spinlock (ATOM 1) | once (ATOM 2), arc (ATOM 4), spsc (ATOM 5), seqlock (ATOM 6), mpmc (ATOM 7) | `cap-runtime` |
| Wakers | block-on (ASYNC 1) | executor (ASYNC 2), async-channel (ASYNC 3), async-primitives (ASYNC 10), io-uring-model (OS 6) | `cap-runtime` |
| Cancellation safety | cancel-safety (ASYNC 6) | graceful-shutdown (ASYNC 7), async-primitives (ASYNC 10), async-retry-hedge (ASYNC 12) | `cap-jobsvc` |
| Trait objects | fn-pipeline (ITER 2) | shape-registry (TRAIT 1), event-bus-any (TRAIT 5), manual-vtable (TRAIT 9), async-trait-object (ASYNC 11) | `cap-redis` |
| Iterator as state machine | iter-adapters (ITER 1) | volcano (QUERY 2), lending-windows (OWN 8), flatten-tree-iter (ITER 5), stream-adapters (ASYNC 8) | `cap-sql` |
| Interior mutability | my-refcell (SMART 2) | once (ATOM 2), rcu-snapshot (ATOM 3), my-rc (SMART 3), bump-arena (MEM 2) | `cap-runtime` |
| Raw memory and MaybeUninit | fixed-vec (UNSAFE 4) | myvec (UNSAFE 5), intrusive-list (UNSAFE 6), bump-arena (MEM 2), spsc (ATOM 5) | `cap-runtime` |
| Bit sets and masks | alg-bit-flags (BIT 1) | bloom (CACHE 4), bitset-roaring (COLL 9), fs-blocks (OS 5), alg-co-bitmask-dp (COMBO 6), alg-co-bt-bitset (COMBO 8) | `cap-fs` |
| Varints and delta coding | varint-codec (BIN 1) | protobuf-wire (BIN 5), gorilla (BIN 7), columnar-encodings (STORE 10), posting-compression (SEARCH 2) | `cap-search` |
| Hashing keys to places | hashmap-chaining (COLL 3) | bloom (CACHE 4), sharded-map (CONC 10), consistent-hash (DIST 8), hash-agg (QUERY 4), hash-join (QUERY 5) | `cap-vectordb` |
| Memoisation | memoize (ITER 3) | alg-regex-count (REC 5), singleflight (CONC 11), peg-packrat (LANG 7), alg-memo-resolver (REC 6) | `cap-lineage` |
| Backtracking | alg-config-matrix (REC 3) | alg-rota (REC 4), unification (LANG 10), linearizability (DIST 16), alg-co-bt-bitset (COMBO 8) | `cap-lease` |
| Graph traversal | alg-reach (GRAPH 2) | alg-order (GRAPH 4), mark-sweep (MEM 5), dataflow (LANG 6), aho-corasick (SEARCH 8), lin-graph (LIN 7) | `cap-lineage` |
| Topological order and cycle reports | alg-order (GRAPH 4) | serializability-checker (TXN 2), spreadsheet (PAT 15), lin-recompute (LIN 8), alg-co-dag-dp (COMBO 4) | `cap-jobsched` |
| Dynamic programming states | alg-dp-1d (DP 1) | alg-dp-strings (DP 4), alg-co-dag-dp (COMBO 4), join-order (QUERY 13), alg-co-tree-dp (COMBO 7) | `cap-optimizer` |
| Greedy with proof or counterexample | alg-gr-select (GRD 1) | alg-gr-deadlines (GRD 2), alg-gr-huffman (GRD 3), alg-co-bsearch-greedy (COMBO 1) | `cap-jobsched` |
| Idempotency and deduplication | idempotency-store (RESIL 4) | dist-delivery (DIST 2), outbox (RESIL 7), raft-apply (DIST 20), lin-projections (LIN 5) | `cap-queue` |
| Versions and snapshots | rcu-snapshot (ATOM 3) | versioned-map (TXN 1), mvcc-si (TXN 3), occ (TXN 5), git-objects (LIN 3), lin-graph (LIN 7) | `cap-gitstore` |
| Logical time and causality | lamport-vclock (DIST 6) | crdt-registers (CRDT 2), quorum-kv (DIST 10), crdt-causal-context (CRDT 5), crdt-opbased (CRDT 9) | `cap-collab` |
| Merge laws and convergence | crdt-counters (CRDT 1) | crdt-set (CRDT 3), crdt-map (CRDT 7), crdt-delta (CRDT 8), text-crdt (CRDT 11) | `cap-collab` |
| Majority quorums | quorum-kv (DIST 10) | raft-core (DIST 17), raft-election (DIST 18), raft-apply (DIST 20) | `cap-raftkv` |
| Leases and fencing tokens | lease-fencing (DIST 7) | dist-sharding (DIST 9), raft-reads (DIST 23) | `cap-lease` |
| Escaping and validating untrusted input | sec-safe-path (SEC 1) | sec-injection (SEC 2), sec-egress (SEC 7), sbx-command-policy (SBX 2) | `cap-backend` |
| Metrics and traces | mon-metrics (MON 2) | mon-spans (MON 4), load-balancer (RESIL 5), mon-slo (MON 5), mon-anomaly (MON 8) | `cap-backend` |
| Approximate answers with bounds | bloom (CACHE 4) | bloom-sstable (STORE 7), count-min (CACHE 5), hll (CACHE 6), cardinality (QUERY 12), knn (SEARCH 9), ivf (SEARCH 13) | `cap-vectordb` |

### Transfer chains

One idea returning in new settings. Spoiler warning as above.

1. **Index-linked list:** linked-list-safe, then lru (recency order), lru-k and buffer-pool (victim choice), timing-wheel (slots of timers).
2. **Heap:** binary-heap, then job-scheduler (what is due next), compaction and external-sort (k-way merge), indexed-pq (urgency changes), alg-routes (nearest unsettled node), alg-co-greedy-heap (two queues at once).
3. **Ring buffer:** ring-deque, then hit-counter (time buckets), spsc (two threads, one index each), mon-flight-recorder (overwrite oldest), io-uring-model (submission and completion).
4. **Need more bytes:** varint-codec and length-codec, then resp, http-request, cancel-safety (the same buffer under select!), json-streaming.
5. **Append-only log:** wal, then memtable-flush, partitioned-log, lin-event-store, raft-replication, raft-persist.
6. **Injected time:** fake-clock, then rate-limiter, ttl-kv, job-scheduler, lease-fencing, sim-network, raft-election.
7. **Model-based testing:** write-tests, then shrinker, test-differential, crdt-convergence, sim-invariant-search, linearizability.
8. **Disjoint sets:** union-find, then lsh-dedupe (cluster pairs), hindley-milner (solve type equations), alg-co-offline-dsu (answer history questions).
9. **Versions:** versioned-map, then rcu-snapshot, mvcc-si, occ, git-objects, lin-graph.
10. **Waker:** block-on, executor, async-channel, async-primitives, register-recheck.

## 5. Series (the main catalogue)

Fields: the **new idea** is the one thing a part adds. **After this** says what you can then do and where it applies. **Required** is at most two direct prerequisites; **Helpful** is optional; **Used again in** is computed from the other parts. *Reuses* is the Required and Helpful lists together. `Tracks` are the existing anneal tracks (or planned codes) whose knowledge the part assumes. Preparation parts added from the capstone readings are marked *prep*; their briefs are shorter (no trade-off line, no project sentence).

### OWN: Ownership and API design

Own, borrow and move data in designs of growing size. *Core series, 8 parts, about 9 h. Ends in `cap-lox`.* Alongside it: `iv-fake-clock` (TEST 1).

#### OWN 1. Bank accounts with two mutable borrows at once

`iv-split-borrow-bank` · 45 min · easy · P2 · Tracks: L2

- **New idea:** two mutable borrows of one collection.
- **Core:** Accounts in a Vec and a map: transfer(a, b, amount) holding &mut to both, with typed errors.
- **Follow-up 1:** Batch of transfers, all or nothing. *Test:* random batches against a clone-and-swap model.
- **Follow-up 2:** transfer to self, duplicate ids in a batch. *Test:* edge tests.
- **Follow-up 3:** Audit log that borrows account names without cloning. *Test:* allocation counting.
- **Trade-offs:** get_disjoint_mut vs split_at_mut vs indices. **Concepts:** disjoint borrows, error enum, atomicity.
- **After this:** you can update two entries safely. **In projects:** account and game-state updates.
- **Required first:** nothing. **Used again in:** `iv-linked-list-safe` (OWN 2), `iv-graph-ownership` (OWN 6), `iv-bvh-flat` (COLL 17), `iv-bank-transfer-concurrent` (CONC 3).
- **Cookbook:** related `borrowck-patterns`, related `std-hashmap-entry-api-patterns`. **Status:** to be written (partly practised by `l2-get-disjoint-mut`). **Runner:** gradable today.

#### OWN 2. Linked lists the Rust way

`iv-linked-list-safe` · 60 min · medium · P2 · Tracks: L1 L2

- **New idea:** owning links moved with take.
- **Core:** A Box-based stack and queue with push, pop, reverse and an iterative Drop.
- **Follow-up 1:** iter, iter_mut, into_iter. *Test:* model tests.
- **Follow-up 2:** Reverse in groups of k and merge two sorted lists. *Test:* random against Vec.
- **Follow-up 3:** A million nodes dropped without overflow. *Test:* scale test.
- **Trade-offs:** Why a doubly linked list is not natural here. **Concepts:** Option<Box>, take, cursors, Drop.
- **After this:** you can write recursive owned structures. **In projects:** free lists and undo chains.
- **Required first:** nothing. **Helpful:** `iv-split-borrow-bank` (OWN 1). **Used again in:** `iv-undo-stack` (OWN 4), `iv-flatten-tree-iter` (ITER 5), `iv-intrusive-list` (UNSAFE 6), `iv-lru` (CACHE 1).
- **Cookbook:** similar\* `linked-lists-in-rust` (reverse, k-group, merge and iterative Drop among its questions), similar `implement-std-from-scratch`. **Status:** to be written (partly practised by `l1-box-linked-stack`, `l2-mut-cursor-loop`). **Runner:** gradable today.

#### OWN 3. Zero-copy record views over one buffer

`iv-view-splitter` · 60 min · medium · P2 · Tracks: L2 L3

- **New idea:** results that borrow from the input.
- **Core:** Parse "k=v" lines into a Records<'a> whose keys and values borrow the input. Lookup by key without allocating.
- **Follow-up 1:** Results of lookups must outlive the Records value. *Test:* a test that drops Records and keeps the &'a str (compiles only with the right lifetimes).
- **Follow-up 2:** An owned form via Cow when a value needs unescaping. *Test:* allocation counting: no allocation for clean input.
- **Follow-up 3:** Split the view into disjoint mutable halves over &mut [u8]. *Test:* writes through both halves at once.
- **Trade-offs:** One lifetime or two; when to return owned data. **Concepts:** lifetimes, borrowing, Cow, split_at_mut.
- **After this:** you can design zero-copy APIs. **In projects:** config and log parsing.
- **Required first:** nothing. **Used again in:** `iv-scoped-callbacks` (OWN 7), `iv-lending-windows` (OWN 8), `iv-tokenizer-borrowed` (PARSE 2).
- **Cookbook:** similar `zero-copy-lexer`, related `cow`, related `lifetimes`, related `generics-lifetimes-structs`. **Status:** to be written (partly practised by `l3-zero-copy-tokenizer`, `l3-binary-frame-parser`). **Runner:** gradable today.

#### OWN 4. Editor with undo and redo

`iv-undo-stack` · 60 min · medium · P1 · Tracks: L1 L2

- **New idea:** ownership moves into a history of commands.
- **Core:** A text buffer with insert(at, text) and delete(range), plus undo() and redo(). Ownership of removed text moves into the history; nothing is cloned that does not have to be.
- **Follow-up 1:** A new edit after undo drops the redo branch. *Test:* random op streams against a model that keeps whole-string snapshots.
- **Follow-up 2:** Coalesce consecutive single-character typing into one undo step. *Test:* scripted sequences with a cursor jump in between.
- **Follow-up 3:** History limit in bytes, oldest first. *Test:* a counting wrapper checks retained bytes after each op.
- **Trade-offs:** Command with inverse vs snapshot vs piece table; where the memory goes. **Concepts:** command pattern, mem::take, enum design.
- **After this:** you can add undo to any editor. **In projects:** editors and migration tools.
- **Required first:** nothing. **Helpful:** `iv-linked-list-safe` (OWN 2). **Used again in:** `iv-text-gap-buffer` (OWN 5), `iv-board-game` (PAT 4), `iv-crdt-undo` (CRDT 13).
- **Cookbook:** similar\* `undo-redo-editors` (command pattern, coalescing and a model test are among five designs), related `design-patterns-high`. **Status:** to be written. **Runner:** gradable today.

#### OWN 5. Gap buffer for a text editor

`iv-text-gap-buffer` · 60 min · medium · P3 · Tracks: S3

- **New idea:** a layout chosen around where edits happen.
- **Core:** A buffer of chars with a cursor: move, insert, delete left and right, to_string. Inserts at the cursor are amortised O(1).
- **Follow-up 1:** Line and column of the cursor. *Test:* compared with a String model that rescans.
- **Follow-up 2:** UTF-8 byte storage that never splits a char. *Test:* random Unicode text, every result must be valid.
- **Follow-up 3:** Convert to a piece table and keep the same tests. *Test:* the same model suite.
- **Trade-offs:** Gap buffer vs rope vs piece table: where each is O(n). **Concepts:** gap buffer, UTF-8 boundaries, copy_within.
- **After this:** you can pick a text structure by edit pattern. **In projects:** editors and REPLs.
- **Required first:** nothing. **Helpful:** `iv-undo-stack` (OWN 4).
- **Cookbook:** related `undo-redo-editors`. **Status:** to be written. **Runner:** gradable today.

#### OWN 6. A graph with removable nodes and stable handles

`iv-graph-ownership` · 60 min · medium · P1 · Tracks: F3

- **New idea:** indices instead of references for shared structure.
- **Core:** add_node, add_edge, neighbours, remove_node over a Vec arena with typed ids.
- **Follow-up 1:** Stale ids are detected after slot reuse. *Test:* random add/remove against a HashMap model.
- **Follow-up 2:** Iterate neighbours while mutating others. *Test:* borrow-friendly API tests.
- **Follow-up 3:** Compact the arena and remap ids. *Test:* structure preserved.
- **Trade-offs:** Rc<RefCell> vs indices vs generational keys. **Concepts:** arena, newtype ids, generations.
- **After this:** you can model graphs without Rc. **In projects:** compilers and schedulers.
- **Required first:** nothing. **Helpful:** `iv-split-borrow-bank` (OWN 1). **Used again in:** `iv-file-system-tree` (PAT 11), `iv-spreadsheet` (PAT 15), `iv-slab` (MEM 1), `iv-mark-sweep` (MEM 5), `iv-trie` (COLL 6), `iv-tree-interpreter` (LANG 4), `iv-git-objects` (LIN 3), `iv-lin-graph` (LIN 7), `iv-alg-graph-repr` (GRAPH 1).
- **Cookbook:** related `arena-ids`, similar `slab-generational`, related `index-graph`. **Status:** to be written (partly practised by `f3-generational-slab`). **Runner:** gradable today.

#### OWN 7. Event handlers that borrow from the caller

`iv-scoped-callbacks` · 90 min · hard · P2 · Tracks: L3 L4

- **New idea:** trait objects with a borrow lifetime.
- **Core:** A Dispatcher<'a, E> storing Box<dyn FnMut(&E) + 'a>; handlers may borrow local counters.
- **Follow-up 1:** Handler that accepts a borrowed event with any lifetime (HRTB). *Test:* register a function that returns nothing from &str of a temporary.
- **Follow-up 2:** Unsubscribe by token while keeping the borrows sound. *Test:* counters readable after the dispatcher is dropped.
- **Follow-up 3:** Make it Send so it can move to a scoped thread. *Test:* compile-checked use with std::thread::scope.
- **Trade-offs:** 'static + Arc vs scoped lifetimes. **Concepts:** trait object lifetimes, HRTB, variance, FnMut.
- **After this:** you can accept callbacks that capture locals. **In projects:** event systems and visitors.
- **Required first:** nothing. **Helpful:** `iv-view-splitter` (OWN 3). **Used again in:** `iv-lending-windows` (OWN 8).
- **Cookbook:** related `closures-hof`, related `hrtb-gats`, related `any-event-bus`. **Status:** to be written (partly practised by `l3-fix-box-dyn-fn-borrows`). **Runner:** gradable today.

#### OWN 8. A lending iterator over mutable windows

`iv-lending-windows` · 90 min · hard · P2 · Tracks: L3 L4

- **New idea:** items that borrow from the iterator itself.
- **Core:** Define a LendingIterator trait with a generic associated type and implement windows_mut(slice, n).
- **Follow-up 1:** A for_each and a fold on the trait. *Test:* prefix sums computed in place equal a naive version.
- **Follow-up 2:** Adapters map and filter. *Test:* compile and behave on random slices; note which signatures fail.
- **Follow-up 3:** Explain why std. *Test:* iter::Iterator cannot express it :: discussion.
- **Trade-offs:** GATs vs a closure-based with_windows. **Concepts:** GAT, HRTB, borrowing.
- **After this:** you can say when std Iterator cannot work. **In projects:** streaming decoders.
- **Required first:** `iv-view-splitter` (OWN 3). **Helpful:** `iv-scoped-callbacks` (OWN 7).
- **Cookbook:** similar `hrtb-gats`. **Status:** to be written (partly practised by `l4-lending-iterator`). **Runner:** gradable today.

### ITER: Closures and iterators

Make laziness and closures your own tools. *Core series, 5 parts, about 6 h. Ends in `cap-sql`.* Alongside it: `iv-write-tests` (TEST 2).

#### ITER 1. Write Map, Filter, Take, Zip, Chain and Peekable

`iv-iter-adapters` · 60 min · medium · P1 · Tracks: S6

- **New idea:** an iterator adapter is a struct that wraps another.
- **Core:** The six adapters plus an extension trait so they chain from any iterator.
- **Follow-up 1:** size_hint that is never wrong. *Test:* random pipelines, hint bounds checked against the real count at every step.
- **Follow-up 2:** DoubleEndedIterator for Map and Chain. *Test:* alternate next and next_back against a VecDeque model.
- **Follow-up 3:** FusedIterator behaviour. *Test:* a source that yields after None.
- **Trade-offs:** Laziness; where Peekable stores its item. **Concepts:** iterator adapters, extension traits, size_hint.
- **After this:** you can write any adapter. **In projects:** every data pipeline.
- **Required first:** nothing. **Used again in:** `iv-peekmore-chunks` (ITER 4), `iv-flatten-tree-iter` (ITER 5), `iv-stream-adapters` (ASYNC 8), `iv-io-adapters` (IO 4), `iv-volcano` (QUERY 2).
- **Cookbook:** similar\* `implement-std-from-scratch` (its Q8 builds these six), related `iterator-adapters`. **Status:** to be written (partly practised by `l4-iterator-extension`). **Runner:** gradable today.

#### ITER 2. A middleware pipeline from closures

`iv-fn-pipeline` · 60 min · medium · P2 · Tracks: L4

- **New idea:** composing boxed closures into layers.
- **Core:** Compose Fn(Request) -> Response with layers added at run time: logging, auth, timing.
- **Follow-up 1:** A layer may short-circuit. *Test:* inner handler call counting.
- **Follow-up 2:** FnMut stateful layer (request counter) shared safely. *Test:* used from 8 threads.
- **Follow-up 3:** The same pipeline built statically with generics. *Test:* identical behaviour tests.
- **Trade-offs:** Box<dyn Fn> vs generic nesting; Fn vs FnMut vs FnOnce. **Concepts:** closures, boxing, chain of responsibility.
- **After this:** you can build middleware chains. **In projects:** HTTP and job middleware.
- **Required first:** nothing. **Used again in:** `iv-memoize` (ITER 3), `iv-event-bus-any` (TRAIT 5), `iv-parser-combinators` (PARSE 16), `iv-middleware-tower` (WEB 4).
- **Cookbook:** similar `design-patterns-high`, similar `axum-middleware`, similar `tower-service-layer`. **Status:** to be written. **Runner:** gradable today.

#### ITER 3. A memoising wrapper for any function

`iv-memoize` · 60 min · medium · P2 · Tracks: L4 S7

- **New idea:** a closure that owns a cache.
- **Core:** memoize(f) -> impl FnMut(K) -> V for K: Hash + Eq + Clone.
- **Follow-up 1:** Recursive functions that call the memoised version. *Test:* fib(90), call counting.
- **Follow-up 2:** Bounded cache with eviction. *Test:* call counts against a model.
- **Follow-up 3:** Thread-safe version computing each key once. *Test:* 16 threads, one computation per key.
- **Trade-offs:** How recursion and borrowing collide. **Concepts:** closures, RefCell, entry API, single flight.
- **After this:** you can cache any pure function. **In projects:** resolvers and renderers.
- **Required first:** nothing. **Helpful:** `iv-fn-pipeline` (ITER 2). **Used again in:** `iv-singleflight` (CONC 11), `iv-peg-packrat` (LANG 7), `iv-lin-recompute` (LIN 8), `iv-alg-regex-count` (REC 5).
- **Cookbook:** related `closures-hof`, related `dp-memoization`, similar `concurrency-interview-questions` (its Q22 is single flight without a cache). **Status:** to be written. **Runner:** gradable today.

#### ITER 4. Group-by and chunking adapters

`iv-peekmore-chunks` · 60 min · medium · P3 · Tracks: S6

- **New idea:** grouping needs one item of lookahead.
- **Core:** chunk_by_key over any iterator, lazy, yielding Vec groups.
- **Follow-up 1:** windows over a non-slice iterator. *Test:* compared with slice windows.
- **Follow-up 2:** merge_sorted of two iterators. *Test:* proof of laziness with a panicking source after the needed prefix.
- **Follow-up 3:** interleave and dedup_by_count. *Test:* random inputs against naive.
- **Trade-offs:** Returning Vec vs borrowing group iterators. **Concepts:** laziness, Peekable, VecDeque buffer.
- **After this:** you can group sorted streams lazily. **In projects:** report and merge pipelines.
- **Required first:** `iv-iter-adapters` (ITER 1). **Used again in:** `iv-compaction` (STORE 6).
- **Cookbook:** similar `std-iterator-adapters-lazy`. **Status:** staged draft: `s6-merge-sorted-streams`. **Runner:** gradable today.

#### ITER 5. Iterators over a tree and a nested list

`iv-flatten-tree-iter` · 90 min · medium · P2 · Tracks: S6 L3

- **New idea:** recursion turned into iterator state.
- **Core:** Lazy in-order iterator over a binary tree with an explicit stack; a flatten over nested enums.
- **Follow-up 1:** Double-ended in-order. *Test:* mix both ends, never yield twice.
- **Follow-up 2:** iter_mut. *Test:* add one to every node without unsafe.
- **Follow-up 3:** A tree 1e6 deep must not overflow. *Test:* skewed tree, including Drop.
- **Trade-offs:** Recursion vs explicit stack; Drop recursion. **Concepts:** iterator state, explicit stack, borrowing.
- **After this:** you can iterate any tree lazily. **In projects:** file and syntax trees.
- **Required first:** `iv-iter-adapters` (ITER 1). **Helpful:** `iv-linked-list-safe` (OWN 2). **Used again in:** `iv-dir-walker` (IO 6).
- **Cookbook:** similar `binary-trees-in-rust`, similar `std-iterator-sources-custom`. **Status:** to be written. **Runner:** gradable today.

### TRAIT: Traits, generics and public APIs

Choose between enums, generics and trait objects, then publish an API. *Core series, 10 parts, about 11 h. Ends in `cap-httpserver`.*

#### TRAIT 1. A plugin registry, static and dynamic

`iv-shape-registry` · 60 min · medium · P2 · Tracks: L4

- **New idea:** dyn Trait and what object safety forbids.
- **Core:** trait Command { name, run }; a registry of Box<dyn Command> with dispatch by name and a typed error.
- **Follow-up 1:** A generic register_fn for closures through a blanket impl. *Test:* register closures and structs side by side.
- **Follow-up 2:** A method that needs Self must stay off the vtable (where Self: Sized). *Test:* the registry still compiles as dyn.
- **Follow-up 3:** Clone the whole registry. *Test:* clone_box, then mutate the copy only.
- **Trade-offs:** enum vs dyn vs generics; what object safety forbids. **Concepts:** dyn dispatch, blanket impl, object safety, clone_box.
- **After this:** you can design plugin points. **In projects:** command and handler registries.
- **Required first:** nothing. **Used again in:** `iv-heterogeneous-store` (TRAIT 4), `iv-event-bus-any` (TRAIT 5), `iv-library-api` (TRAIT 8), `iv-manual-vtable` (TRAIT 9), `iv-async-trait-object` (ASYNC 11).
- **Cookbook:** similar `type-erasure-vtable`, related `dyn-vs-generic`, related `design-patterns-high`. **Status:** to be written (partly practised by `l4-plugin-logger`, `l4-where-self-sized`). **Runner:** gradable today.

#### TRAIT 2. Money without mixing currencies

`iv-units-newtypes` · 60 min · medium · P2 · Tracks: L5 S8

- **New idea:** types that make mixed units impossible.
- **Core:** Money<C> over integer minor units: add, subtract, display, parse, with currency in the type.
- **Follow-up 1:** Checked multiply by a rate with explicit rounding modes. *Test:* tables of half cases for each mode.
- **Follow-up 2:** Split an amount into n parts losing nothing. *Test:* sum of parts equals the whole for random inputs.
- **Follow-up 3:** A runtime-checked AnyMoney for data from outside. *Test:* TryFrom into Money<C>.
- **Trade-offs:** Type-level vs value-level currency. **Concepts:** PhantomData, newtype, operator traits, integer overflow.
- **After this:** you can encode domain rules in types. **In projects:** money and ids.
- **Required first:** nothing. **Used again in:** `iv-generic-matrix` (TRAIT 3), `iv-typestate-conn` (TRAIT 7), `iv-bank-ledger` (PAT 6), `iv-sec-injection` (SEC 2).
- **Cookbook:** similar `decimal-fixed-point-money`, related `phantomdata-typed`, related `newtype-ids`. **Status:** to be written (partly practised by `l5-phantom-units-of-measure`). **Runner:** gradable today.

#### TRAIT 3. A generic matrix over a numeric trait

`iv-generic-matrix` · 60 min · medium · P3 · Tracks: L4 L5

- **New idea:** operator traits with bounds on impls.
- **Core:** Matrix<T> with Index, Add, Mul for T: your own Num trait; shape errors as values.
- **Follow-up 1:** const-generic fixed shapes where bad products do not compile. *Test:* positive tests.
- **Follow-up 2:** &a * &b without cloning. *Test:* types that are not Copy.
- **Follow-up 3:** Display with column alignment. *Test:* golden strings.
- **Trade-offs:** Trait bounds on impls vs on the struct. **Concepts:** operator overloading, const generics, trait design.
- **After this:** you can make a type feel built in. **In projects:** numeric libraries.
- **Required first:** nothing. **Helpful:** `iv-units-newtypes` (TRAIT 2). **Used again in:** `iv-bigint` (NUM 1), `iv-ray-intersection` (NUM 3).
- **Cookbook:** similar `implementing-std-traits`, related `const-and-generics-advanced`, similar\* `implement-std-from-scratch` (its Q15 formats and multiplies a matrix). **Status:** to be written (partly practised by `l4-matrix-operators`, `l5-shape-typed-matrix`). **Runner:** gradable today.

#### TRAIT 4. A type map and property bag

`iv-heterogeneous-store` · 45 min · medium · P3 · Tracks: L4

- **New idea:** TypeId keys and downcasting.
- **Core:** Extensions: insert<T>, get<T>, get_mut<T>, remove<T>.
- **Follow-up 1:** A string-keyed bag with typed get. *Test:* wrong type gives an error, not a panic.
- **Follow-up 2:** Make it Clone. *Test:* clone then diverge.
- **Follow-up 3:** Replace SipHash for TypeId keys with a pass-through hasher. *Test:* counting hasher.
- **Trade-offs:** Any vs generics vs enum. **Concepts:** TypeId, Box<dyn Any>, BuildHasher.
- **After this:** you can store values by type. **In projects:** request extensions.
- **Required first:** nothing. **Helpful:** `iv-shape-registry` (TRAIT 1). **Used again in:** `iv-event-bus-any` (TRAIT 5).
- **Cookbook:** related `type-erasure-vtable`, related `fast-hashing`. **Status:** exists: `l3-static-bound-vs-static-ref`. **Runner:** gradable today.

#### TRAIT 5. A typed event bus on TypeId

`iv-event-bus-any` · 60 min · medium · P1 · Tracks: L4

- **New idea:** runtime dispatch that is typed at the call site.
- **Core:** publish<E: 'static>(e) calls every subscriber of E; subscribe<E>(closure).
- **Follow-up 1:** Unsubscribe by handle, including from inside a handler. *Test:* reentrancy test that would panic with a RefCell guard held.
- **Follow-up 2:** Events published from a handler are queued, not recursed. *Test:* order of deliveries is asserted.
- **Follow-up 3:** Priorities among handlers. *Test:* seeded subscriptions against a sorted model.
- **Trade-offs:** Any downcasting vs an enum of events. **Concepts:** TypeId, Any, reentrancy, queueing.
- **After this:** you can decouple publishers from subscribers. **In projects:** app event buses.
- **Required first:** `iv-shape-registry` (TRAIT 1). **Helpful:** `iv-heterogeneous-store` (TRAIT 4), `iv-fn-pipeline` (ITER 2).
- **Cookbook:** similar\* `any-event-bus` (typed publish and subscribe; check unsubscribe yourself), related `design-patterns-high`. **Status:** to be written (partly practised by `l3-static-bound-vs-static-ref`). **Runner:** gradable today.

#### TRAIT 6. Declarative macros: map!, matches_enum!, vec_of!

`iv-macro-hashmap` · 60 min · medium · P1 · Tracks: L10

- **New idea:** declarative macros and hygiene.
- **Core:** map!{k => v} with trailing commas, evaluating each argument once.
- **Follow-up 1:** Counting macro to pre-size capacity. *Test:* capacity asserted.
- **Follow-up 2:** A tt-muncher for a tiny enum-with-names macro. *Test:* name tables.
- **Follow-up 3:** Hygiene: works where the caller shadows HashMap. *Test:* test module with its own HashMap.
- **Trade-offs:** macro_rules! limits; when a function is better. **Concepts:** repetition, hygiene, $crate.
- **After this:** you can write safe macros. **In projects:** test and config helpers.
- **Required first:** nothing. **Used again in:** `iv-derive-builder` (TRAIT 10).
- **Cookbook:** similar\* `macro-rules` (repetitions and tt-munchers; its max_of! evaluates arguments twice), related `metaprogramming-macros-and-staging`. **Status:** staged draft: `l10-fix-macro-hygiene`. **Runner:** gradable today.

#### TRAIT 7. A connection API where misuse does not compile

`iv-typestate-conn` · 60 min · medium · P2 · Tracks: L5

- **New idea:** states as types.
- **Core:** Connection<Closed|Open|Authed> with consuming transitions; send exists only on Authed.
- **Follow-up 1:** Failed login returns the Open connection back inside the error. *Test:* tests recover and retry.
- **Follow-up 2:** Builder with a required URL. *Test:* positive tests only.
- **Follow-up 3:** Seal the state trait. *Test:* discussion, graded only by review.
- **Trade-offs:** Typestate vs runtime enum; what generics cost in signatures. **Concepts:** typestate, PhantomData, sealed trait, ownership transfer.
- **After this:** you can make misuse a compile error. **In projects:** protocol clients and builders.
- **Required first:** nothing. **Helpful:** `iv-units-newtypes` (TRAIT 2). **Used again in:** `iv-library-api` (TRAIT 8), `iv-sbx-capabilities` (SBX 1).
- **Cookbook:** similar\* `typestate-protocol` (states, sealed trait and login), similar `typestate-builder`. **Status:** to be written (partly practised by `l5-typestate-http-builder`, `l5-newtype-typestate`). **Runner:** ok; compile-fail cases cannot be graded.

#### TRAIT 8. Evolve a small public API

`iv-library-api` · 60 min · medium · P3 · Tracks: L9

- **New idea:** visibility, sealing and evolution.
- **Core:** A module tree with private fields, a smart constructor, re-exports and a prelude.
- **Follow-up 1:** #[non_exhaustive] error enum and a builder. *Test:* external-style tests use only the public API.
- **Follow-up 2:** Sealed trait. *Test:* reviewed, not graded.
- **Follow-up 3:** IntoIterator and standard trait set. *Test:* tests require each trait.
- **Trade-offs:** What is a breaking change. **Concepts:** modules, visibility, API guidelines.
- **After this:** you can ship a crate without breaking users. **In projects:** any shared library.
- **Required first:** `iv-typestate-conn` (TRAIT 7). **Helpful:** `iv-shape-registry` (TRAIT 1). **Used again in:** `iv-derive-builder` (TRAIT 10).
- **Cookbook:** similar `library-api-polish`, related `modules-visibility`. **Status:** to be written. **Runner:** ok; semver checks cannot be graded.

#### TRAIT 9. Type erasure with a hand-written vtable

`iv-manual-vtable` · 90 min · hard · P3 · Tracks: Y2

- **New idea:** what a trait object is made of.
- **Core:** An erased task type built from any FnOnce, using a struct of function pointers and a raw data pointer, no dyn.
- **Follow-up 1:** Drop exactly once whether run or not. *Test:* drop-counting payloads.
- **Follow-up 2:** Inline small payloads. *Test:* allocation counting.
- **Follow-up 3:** Send bound carried by the constructor. *Test:* compile-checked thread use.
- **Trade-offs:** What the compiler's vtable gives you; soundness obligations. **Concepts:** vtable, raw pointers, Drop, small-object optimisation.
- **After this:** you can read and write type erasure. **In projects:** async runtimes and arenas.
- **Required first:** `iv-shape-registry` (TRAIT 1). **Helpful:** `iv-my-refcell` (SMART 2).
- **Cookbook:** similar `type-erasure-vtable`. **Status:** to be written. **Runner:** ok; Miri would be needed to check soundness.

#### TRAIT 10. derive(Builder) as a procedural macro

`iv-derive-builder` · 120 min · hard · P3 · Tracks: L10

- **New idea:** code that generates code.
- **Core:** Derive a builder for a struct with named fields.
- **Follow-up 1:** Optional fields and defaults. *Test:* behaviour tests.
- **Follow-up 2:** Clear compile errors for enums and tuple structs. *Test:* needs trybuild.
- **Follow-up 3:** Repeated-element setters. *Test:* behaviour tests.
- **Trade-offs:** syn parsing; span-accurate errors. **Concepts:** proc macros, token streams.
- **After this:** you can read proc-macro crates. **In projects:** serde-style derives.
- **Required first:** `iv-macro-hashmap` (TRAIT 6). **Helpful:** `iv-library-api` (TRAIT 8).
- **Cookbook:** similar `cb-macros`, similar `cb-macros-demo`. **Status:** to be written. **Runner:** needs multi-file grading: a proc-macro crate, and syn and quote are not in docker/deps.

### ERR: Errors and panic safety

Errors that keep their cause, and systems that survive bugs. *Core series, 3 parts, about 3 h. Ends in `cap-jobsvc`.*

#### ERR 1. Compiler-style error rendering

`iv-diagnostics-renderer` · 45 min · medium · P2 · Tracks: S2

- **New idea:** byte spans turned into line and column.
- **Core:** Given source and a byte span, print line, column and a caret underline.
- **Follow-up 1:** Multi-line spans and tabs. *Test:* golden outputs.
- **Follow-up 2:** Columns counted in chars, not bytes. *Test:* non-ASCII sources.
- **Follow-up 3:** Several diagnostics sorted and merged on one line. *Test:* golden outputs.
- **Trade-offs:** Byte offsets vs line/column storage. **Concepts:** spans, UTF-8, binary search over line starts.
- **After this:** you can report errors a user can act on. **In projects:** compilers and config tools.
- **Required first:** nothing. **Used again in:** `iv-template-engine` (TEXT 5), `iv-ini-toml` (PARSE 4), `iv-error-recovery` (PARSE 18), `iv-typecheck-simple` (LANG 3).
- **Cookbook:** similar\* `diagnostics-spans` (single span with caret), similar `expression-parsing-three-ways`. **Status:** to be written. **Runner:** gradable today.

#### ERR 2. A config loader with a layered error type

`iv-error-layers` · 60 min · medium · P1 · Tracks: L8

- **New idea:** a typed error with a source chain, and context above it.
- **Core:** Load key=value settings into a struct; one library error enum with line numbers and sources.
- **Follow-up 1:** Collect every error, not only the first. *Test:* multi-error inputs compared by position.
- **Follow-up 2:** Classify retryable vs fatal. *Test:* a method asserted per variant.
- **Follow-up 3:** Application layer adds context and can still downcast. *Test:* walk the source chain.
- **Trade-offs:** thiserror vs anyhow vs Box<dyn Error>. **Concepts:** error design, source chain, From, downcast.
- **After this:** you can design library and app errors. **In projects:** every service.
- **Required first:** nothing. **Used again in:** `iv-panic-boundary` (ERR 3), `iv-bank-ledger` (PAT 6), `iv-axum-api` (WEB 3).
- **Cookbook:** similar `error-chain-retryable`, similar `anyhow-context`, related `error-enum`. **Status:** staged draft: `l8-import-errors`, `l8-fix-lost-context`. **Runner:** ok (thiserror and anyhow are vendored).

#### ERR 3. Run untrusted jobs without dying

`iv-panic-boundary` · 60 min · medium · P2 · Tracks: C1

- **New idea:** unwinding as a controlled boundary.
- **Core:** run_all(jobs) that catches panics and returns a Result per job with the panic message.
- **Follow-up 1:** Poisoned shared state is detected and rebuilt. *Test:* a job that panics while holding a lock.
- **Follow-up 2:** Drop guards roll back partial writes during unwinding. *Test:* state compared after the panic.
- **Follow-up 3:** No double panic in Drop. *Test:* test with a failing guard.
- **Trade-offs:** catch_unwind limits; panic=abort. **Concepts:** panic safety, catch_unwind, poisoning, guards.
- **After this:** you can isolate plugin or job failures. **In projects:** workers and servers.
- **Required first:** `iv-error-layers` (ERR 2). **Used again in:** `iv-ffi-wrapper` (UNSAFE 3), `iv-thread-pool` (CONC 9), `iv-tokio-job-outcome` (ASYNC 13).
- **Cookbook:** similar `std-panic-process-exit-catch`. **Status:** to be written (partly practised by `l1-rollback-guard`). **Runner:** gradable today.

### TEXT: Strings, Unicode and text

Bytes, chars and borrowed output. *Core series, 6 parts, about 6 h. Ends in `cap-typeahead`.* Alongside it: `iv-fuzz-harness` (TEST 5).

#### TEXT 1. UTF-8 and UTF-16 codec by hand

`iv-utf8-codec` · 60 min · medium · P2 · Tracks: S2 F5

- **New idea:** how UTF-8 is laid out and validated.
- **Core:** Encode and strictly decode UTF-8, rejecting overlongs and surrogates.
- **Follow-up 1:** Streaming decoder across chunk boundaries. *Test:* every split point of random text.
- **Follow-up 2:** Lossy mode matching std replacement placement. *Test:* differential against from_utf8_lossy.
- **Follow-up 3:** UTF-16 with surrogate pairs. *Test:* differential against encode_utf16.
- **Trade-offs:** Validation tables vs branches. **Concepts:** bit manipulation, state machine, streaming.
- **After this:** you can handle partial and invalid input. **In projects:** network text decoding.
- **Required first:** nothing. **Used again in:** `iv-string-builder` (TEXT 2).
- **Cookbook:** similar\* `encodings-codecs` (UTF-8 and UTF-16 among its codecs), related `std-char-ascii-unicode`. **Status:** to be written. **Runner:** gradable today.

#### TEXT 2. MyString with its UTF-8 invariant

`iv-string-builder` · 60 min · medium · P2 · Tracks: S2 Y2

- **New idea:** a type whose invariant is its encoding.
- **Core:** A String over Vec<u8>: push, pop, insert at char boundary, Deref to str.
- **Follow-up 1:** Non-boundary indices fail cleanly. *Test:* every byte offset of random Unicode.
- **Follow-up 2:** from_utf8 and from_utf8_lossy. *Test:* differential against std on random bytes.
- **Follow-up 3:** char_indices iterator written by hand. *Test:* differential against std.
- **Trade-offs:** Why indexing by integer is not offered. **Concepts:** UTF-8, invariants, unsafe from_utf8_unchecked.
- **After this:** you can wrap unsafe code in a safe invariant. **In projects:** custom string types.
- **Required first:** `iv-utf8-codec` (TEXT 1).
- **Cookbook:** similar\* `implement-std-from-scratch` (its Q7), related `std-string-building-utf8`. **Status:** to be written. **Runner:** gradable today.

#### TEXT 3. Base64, hex and percent codecs

`iv-base64` · 45 min · easy · P2 · Tracks: F5

- **New idea:** moving bits across symbol boundaries.
- **Core:** Encode and strict decode.
- **Follow-up 1:** Padding and canonical form errors. *Test:* tables.
- **Follow-up 2:** Streaming encoder over Write. *Test:* chunk splits.
- **Follow-up 3:** No intermediate allocation. *Test:* counting.
- **Trade-offs:** Table design. **Concepts:** bit accumulation.
- **After this:** you can write strict codecs. **In projects:** tokens and uploads.
- **Required first:** nothing. **Used again in:** `iv-url-shortener` (PAT 2), `iv-url-parser` (PARSE 6), `iv-cursor-pagination` (WEB 2), `iv-jwt-hmac` (SEC 4).
- **Cookbook:** similar\* `encodings-codecs`. **Status:** to be written. **Runner:** gradable today.

#### TEXT 4. Word wrap and justify

`iv-text-justify` · 45 min · medium · P3 · Tracks: S2

- **New idea:** greedy layout against optimal layout.
- **Core:** Wrap to a width, left-aligned, never splitting a word that fits.
- **Follow-up 1:** Full justification. *Test:* width invariant on random text.
- **Follow-up 2:** Long words and wide characters policy. *Test:* edge tables.
- **Follow-up 3:** Minimise raggedness with DP. *Test:* compare cost with brute force on small inputs.
- **Trade-offs:** Greedy vs DP. **Concepts:** strings, greedy, DP.
- **After this:** you can format text for terminals. **In projects:** CLI output.
- **Required first:** nothing.
- **Cookbook:** related `dp-zoo`. **Status:** to be written. **Runner:** gradable today.

#### TEXT 5. A small template engine

`iv-template-engine` · 90 min · medium · P2 · Tracks: S2 N

- **New idea:** compile a template once and render many times.
- **Core:** {{name}} substitution with escaping, errors with positions.
- **Follow-up 1:** if and for blocks with nesting. *Test:* golden renders and unbalanced-block errors.
- **Follow-up 2:** HTML auto-escaping and a raw marker. *Test:* hostile inputs.
- **Follow-up 3:** Compile once, render many. *Test:* allocation counting on the render path.
- **Trade-offs:** Interpreting the string vs compiling to a tree. **Concepts:** lexing, recursive descent, Cow.
- **After this:** you can build small text languages. **In projects:** email and code generation.
- **Required first:** nothing. **Helpful:** `iv-diagnostics-renderer` (ERR 1).
- <details><summary>Links hidden until asked</summary>helpful: `iv-tokenizer-borrowed` (PARSE 2)</details>
- **Cookbook:** related `markdown-block-parser`, related `cow`. **Status:** to be written (partly practised by `s2-template-formatter`). **Runner:** gradable today.

#### TEXT 6. Normalise text into search keys

`iv-unicode-search-keys` · 45 min · medium · P2 · prep · Tracks: TEXT

- **New idea:** matching works on a folded key while the original text is kept for display.
- **Core:** Fold case, a given accent table, combining marks and whitespace into one key, handling lowercase expansions to several chars.
- **Follow-up 1:** Make normalisation idempotent. *Test:* seeded Unicode strings including dotted capital I ; Map a match in the key back to a byte range of the original for highlighting :: golden tables and char-boundary checks.
- **Required first:** nothing.
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

### SMART: Smart pointers, interior mutability and RAII

Shared ownership and cleanup, then your own versions. *Core series, 4 parts, about 5 h. Ends in `cap-runtime`.*

#### SMART 1. An object pool with guards

`iv-raii-pool` · 60 min · medium · P1 · Tracks: S7 C1

- **New idea:** a guard returns a resource when dropped.
- **Core:** Pool<T>: get() returns a guard that derefs to T and returns it on drop.
- **Follow-up 1:** Capacity limit with blocking and timeout-free try_get. *Test:* 8 threads, max live never above the cap.
- **Follow-up 2:** A broken object can be discarded through the guard. *Test:* creation counts.
- **Follow-up 3:** Panic while borrowed does not return a poisoned object. *Test:* policy asserted.
- **Trade-offs:** Guard lifetime vs Arc-owning guard. **Concepts:** RAII, Drop, Condvar, Deref.
- **After this:** you can manage scarce resources. **In projects:** connection pools.
- **Required first:** nothing. **Used again in:** `iv-my-refcell` (SMART 2), `iv-ambient-context` (SMART 4), `iv-logger` (PAT 9), `iv-ffi-wrapper` (UNSAFE 3), `iv-fixed-vec` (UNSAFE 4), `iv-semaphore` (CONC 5), `iv-spinlock` (ATOM 1), `iv-rcu-snapshot` (ATOM 3), `iv-sbx-quota` (SBX 3), `iv-buffer-pool` (STORE 9), `iv-mon-spans` (MON 4).
- **Cookbook:** similar `design-patterns-low`, similar `design-patterns-high`, related `drop-raii`. **Status:** to be written. **Runner:** gradable today.

#### SMART 2. Cell and RefCell from UnsafeCell

`iv-my-refcell` · 60 min · medium · P1 · Tracks: Y2

- **New idea:** checking borrow rules at run time.
- **Core:** MyCell for Copy values and MyRefCell with borrow and borrow_mut guards.
- **Follow-up 1:** try_borrow errors instead of panics. *Test:* borrow state matrix.
- **Follow-up 2:** Guards release on unwinding. *Test:* panic inside a borrow, then borrow again.
- **Follow-up 3:** Ref. *Test:* map to a field :: lifetime and release tests.
- **Trade-offs:** Why UnsafeCell is the only legal door. **Concepts:** interior mutability, guards, Drop.
- **After this:** you can build interior mutability. **In projects:** observer and cache internals.
- **Required first:** nothing. **Helpful:** `iv-raii-pool` (SMART 1). **Used again in:** `iv-manual-vtable` (TRAIT 9), `iv-my-rc` (SMART 3), `iv-rwlock` (CONC 17).
- **Cookbook:** similar\* `unsafecell-cells` (builds both). **Status:** to be written. **Runner:** ok; Miri would be needed to check soundness.

#### SMART 3. Rc and Weak from scratch

`iv-my-rc` · 120 min · hard · P1 · Tracks: S7 Y2

- **New idea:** two counts: owners and observers.
- **Core:** MyRc<T> with clone, deref, drop and strong_count.
- **Follow-up 1:** MyWeak with upgrade; value dropped at strong zero, allocation freed at weak zero. *Test:* drop counters and cyclic graphs.
- **Follow-up 2:** get_mut and make_mut. *Test:* aliasing scenarios.
- **Follow-up 3:** A parent/child tree with no leak. *Test:* drop count at the end.
- **Trade-offs:** Two counts and why the allocation outlives the value. **Concepts:** refcounting, NonNull, Drop, Weak.
- **After this:** you can explain and write shared ownership. **In projects:** graphs and caches.
- **Required first:** `iv-my-refcell` (SMART 2). **Used again in:** `iv-rcu-snapshot` (ATOM 3), `iv-arc` (ATOM 4).
- **Cookbook:** similar\* `implement-std-from-scratch` (its Q3), similar `own-arc-refcount`. **Status:** staged draft: `s7-fix-observer-leak`. **Runner:** ok; Miri would be needed to check soundness.

#### SMART 4. A thread-local current handle with a restoring guard

`iv-ambient-context` · 45 min · medium · P2 · prep · Tracks: SMART

- **New idea:** an implicit context is installed per thread and restored when a guard drops.
- **Core:** Write enter, current and a free function that uses the installed handle, using thread_local with RefCell.
- **Follow-up 1:** nesting two contexts. *Test:* inner then outer observed ; panic inside a context :: previous value restored after catch_unwind.
- **Required first:** `iv-raii-pool` (SMART 1).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

### PAT: Modelling with Rust (machine coding)

Turn a product description into types, then add rules an interviewer adds. *Core series, 15 parts, about 17 h. Ends in `cap-ledger`.* Alongside it: `iv-fake-clock` (TEST 1), `iv-mon-metrics` (MON 2).

#### PAT 1. Vending machine state machine

`iv-vending-machine` · 45 min · easy · P1 · Tracks: L7

- **New idea:** states and events as enums.
- **Core:** States and events as enums; every illegal transition is an error that keeps the state.
- **Follow-up 1:** Inventory and change-making with limited coins. *Test:* brute force on small coin sets.
- **Follow-up 2:** Replay an event log to rebuild state. *Test:* replay equals live.
- **Follow-up 3:** Table-driven version. *Test:* the same tests.
- **Trade-offs:** enum vs typestate vs table. **Concepts:** state machine, exhaustive match, event replay.
- **After this:** you can model workflows with no illegal moves. **In projects:** order and job state.
- **Required first:** nothing. **Used again in:** `iv-parking-lot` (PAT 5), `iv-elevator` (PAT 12), `iv-actor` (CONC 16), `iv-sans-io` (NET 1), `iv-sbx-supervisor` (SBX 6), `iv-circuit-breaker` (RESIL 3), `iv-saga` (RESIL 8), `iv-decide-evolve` (LIN 10), `iv-alg-dp-state-machine` (DP 6).
- **Cookbook:** similar `generic-state-machine`, similar `state-machine`. **Status:** to be written. **Runner:** gradable today.

#### PAT 2. URL shortener core

`iv-url-shortener` · 45 min · easy · P1 · Tracks: S4

- **New idea:** two-way mapping between ids and codes.
- **Core:** shorten and resolve with base62 ids.
- **Follow-up 1:** Custom aliases and collisions. *Test:* model.
- **Follow-up 2:** Expiry and hit counts with a fake clock. *Test:* model.
- **Follow-up 3:** Non-guessable codes via a reversible permutation. *Test:* round trip over many ids.
- **Trade-offs:** Counter vs hash vs random. **Concepts:** encoding, modular inverse, maps.
- **After this:** you can design id schemes. **In projects:** link and invite services.
- **Required first:** nothing. **Helpful:** `iv-base64` (TEXT 3).
- **Cookbook:** similar\* `systems-design-in-code` (its Q1), related `encodings-codecs`. **Status:** to be written. **Runner:** gradable today.

#### PAT 3. Snake game core

`iv-snake` · 45 min · easy · P3 · Tracks: S5

- **New idea:** state kept in two structures that agree.
- **Core:** move, grow, die on walls or self in O(1) per step.
- **Follow-up 1:** Food placement from a seed. *Test:* reproducible.
- **Follow-up 2:** Wrap-around mode. *Test:* oracle.
- **Follow-up 3:** Replay. *Test:* same states.
- **Trade-offs:** VecDeque plus HashSet. **Concepts:** deque, set, simulation.
- **After this:** you can keep indexes consistent. **In projects:** games and schedulers.
- **Required first:** nothing.
- **Cookbook:** related `std-vec-vecdeque-methods`. **Status:** to be written. **Runner:** gradable today.

#### PAT 4. A turn-based game engine

`iv-board-game` · 60 min · medium · P3 · Tracks: L7

- **New idea:** incremental counters instead of rescans.
- **Core:** Tic-tac-toe on an N x N board with O(1) win detection per move.
- **Follow-up 1:** k-in-a-row on large boards. *Test:* random games against a full-scan oracle.
- **Follow-up 2:** Undo moves. *Test:* counters restored exactly.
- **Follow-up 3:** A second game behind a Game trait. *Test:* shared driver tests.
- **Trade-offs:** Counters vs rescans. **Concepts:** enums, incremental counters, trait design.
- **After this:** you can keep derived state cheap. **In projects:** dashboards and games.
- **Required first:** nothing. **Helpful:** `iv-undo-stack` (OWN 4).
- **Cookbook:** related `procedural-puzzles`. **Status:** to be written. **Runner:** gradable today.

#### PAT 5. Parking lot

`iv-parking-lot` · 60 min · medium · P1 · Tracks: L4

- **New idea:** policy separated from bookkeeping.
- **Core:** Spot sizes, park and leave with tickets, nearest free spot.
- **Follow-up 1:** Fee rules as a strategy with an injected clock. *Test:* fake-clock fee tables.
- **Follow-up 2:** Concurrent entry and exit. *Test:* no double allocation across 8 threads.
- **Follow-up 3:** Multiple floors and a display board. *Test:* counts always equal a recount.
- **Trade-offs:** Free lists per size vs one ordered set. **Concepts:** data modelling, strategy, BTreeSet, locking.
- **After this:** you can add pricing or placement rules later. **In projects:** booking systems.
- **Required first:** `iv-vending-machine` (PAT 1).
- **Cookbook:** related `std-btree-binaryheap-ordered`, related `design-patterns-high`. **Status:** to be written. **Runner:** gradable today.

#### PAT 6. Double-entry bank ledger

`iv-bank-ledger` · 60 min · medium · P1 · Tracks: L8

- **New idea:** invariants kept by balanced entries.
- **Core:** Accounts, balanced postings, no overdraft, typed errors.
- **Follow-up 1:** Idempotent request ids. *Test:* duplicated requests change nothing.
- **Follow-up 2:** Holds and captures. *Test:* available vs posted invariants.
- **Follow-up 3:** Rebuild from the journal. *Test:* replay equals live.
- **Trade-offs:** Integer money; where invariants live. **Concepts:** ledger invariants, idempotency, event replay.
- **After this:** you can build money flows that audit. **In projects:** billing and wallets.
- **Required first:** `iv-error-layers` (ERR 2). **Helpful:** `iv-units-newtypes` (TRAIT 2). **Used again in:** `iv-splitwise` (PAT 7), `iv-outbox` (RESIL 7).
- **Cookbook:** similar `event-sourced-ledger`, similar `decimal-fixed-point-money`. **Status:** to be written. **Runner:** gradable today.

#### PAT 7. Expense sharing

`iv-splitwise` · 60 min · medium · P2 · Tracks: S4

- **New idea:** rounding without losing units, and netting balances.
- **Core:** Add expenses with equal and exact splits; balances.
- **Follow-up 1:** Percent splits losing no cent. *Test:* sums are zero.
- **Follow-up 2:** Simplify debts. *Test:* same net balances, transfer count at most n-1.
- **Follow-up 3:** Edit and delete expenses. *Test:* replay oracle.
- **Trade-offs:** Greedy simplification is not minimal. **Concepts:** rounding, heaps, graphs.
- **After this:** you can split and settle amounts. **In projects:** expense and payout systems.
- **Required first:** `iv-bank-ledger` (PAT 6).
- <details><summary>Links hidden until asked</summary>helpful: `iv-binary-heap` (COLL 4)</details>
- **Cookbook:** related `decimal-fixed-point-money`. **Status:** to be written. **Runner:** gradable today.

#### PAT 8. Feature flag evaluator

`iv-feature-flags` · 60 min · medium · P2 · Tracks: S4

- **New idea:** stable hashing for rollout.
- **Core:** Flags with targeting rules and percentage rollout.
- **Follow-up 1:** Sticky bucketing: raising the percentage only adds users. *Test:* property over many ids.
- **Follow-up 2:** Versioned updates that ignore stale ones. *Test:* reordered update streams.
- **Follow-up 3:** Read-mostly concurrent access with snapshot swap. *Test:* readers never see a mix.
- **Trade-offs:** Hash choice; snapshot vs lock. **Concepts:** hashing, RCU idea, versioning.
- **After this:** you can ship features gradually. **In projects:** any product backend.
- **Required first:** nothing.
- **Cookbook:** similar\* `systems-design-in-code` (its Q15), related `snapshot-cow-rcu`. **Status:** to be written. **Runner:** gradable today.

#### PAT 9. Logger library

`iv-logger` · 60 min · medium · P2 · Tracks: S9 C2

- **New idea:** sinks behind a trait with a drop policy.
- **Core:** Levels, sinks as a trait, formatted records.
- **Follow-up 1:** Async sink on a background thread with bounded queue and drop policy. *Test:* counts of dropped and written add up.
- **Follow-up 2:** Rotation by size over a Write factory. *Test:* in-memory sinks.
- **Follow-up 3:** Flush on drop. *Test:* nothing lost.
- **Trade-offs:** Back-pressure policy. **Concepts:** trait objects, channels, Drop.
- **After this:** you can design pluggable output. **In projects:** logging and telemetry.
- **Required first:** `iv-blocking-queue` (CONC 2). **Helpful:** `iv-raii-pool` (SMART 1).
- **Cookbook:** similar `tracing-logs`, similar\* `systems-design-in-code` (its Q14: a batching agent). **Status:** to be written (partly practised by `l4-plugin-logger`). **Runner:** gradable today.

#### PAT 10. Notification dispatcher

`iv-notification` · 60 min · medium · P2 · Tracks: C2

- **New idea:** delivery policy: retry, failover, dedup.
- **Core:** Send through providers with retries.
- **Follow-up 1:** Failover between providers. *Test:* scripted failures.
- **Follow-up 2:** Dedup by key. *Test:* replayed input sends once.
- **Follow-up 3:** Per-user rate limits and quiet hours with fake clock. *Test:* model.
- **Trade-offs:** At-least-once and idempotency. **Concepts:** retries, strategy, dedup.
- **After this:** you can send things once enough times. **In projects:** alerting and email.
- **Required first:** `iv-retry-backoff` (RESIL 1). **Helpful:** `iv-rate-limiter` (RESIL 2).
- **Cookbook:** similar\* `systems-design-in-code` (its Q6). **Status:** to be written. **Runner:** gradable today.

#### PAT 11. In-memory file system

`iv-file-system-tree` · 60 min · medium · P1 · Tracks: S4 S9

- **New idea:** a recursive enum tree addressed by paths.
- **Core:** mkdir, write, read, ls with absolute paths.
- **Follow-up 1:** Move and recursive delete, no cycles. *Test:* random ops against a path-map model.
- **Follow-up 2:** Glob search. *Test:* matcher oracle.
- **Follow-up 3:** Quotas per directory. *Test:* totals equal recomputation.
- **Trade-offs:** Path parsing; tree ownership. **Concepts:** tree, path parsing, enum nodes.
- **After this:** you can model hierarchies. **In projects:** config trees and virtual files.
- **Required first:** `iv-graph-ownership` (OWN 6). **Helpful:** `iv-trie` (COLL 6). **Used again in:** `iv-unix-namespace` (OS 8), `iv-sbx-vfs-jail` (SBX 4), `iv-crdt-movable-tree` (CRDT 14).
- **Cookbook:** similar `tries-advanced-questions` (its Q11), related `toy-filesystem`. **Status:** to be written. **Runner:** gradable today.

#### PAT 12. Elevator controller

`iv-elevator` · 90 min · hard · P2 · Tracks: L4 L7

- **New idea:** a tick-driven simulation with pluggable policy.
- **Core:** One lift, a step() tick, hall and car requests served in SCAN order.
- **Follow-up 1:** Scheduling policy as a trait (nearest car, SCAN). *Test:* scripted ticks, same trace per policy.
- **Follow-up 2:** Several cars and dispatch. *Test:* no request starves over random loads.
- **Follow-up 3:** Capacity and doors as states. *Test:* invariants after each tick.
- **Trade-offs:** Simulation design; testing with ticks instead of threads. **Concepts:** discrete simulation, state machine, trait strategy.
- **After this:** you can test schedulers without threads. **In projects:** dispatch systems.
- **Required first:** `iv-vending-machine` (PAT 1).
- **Cookbook:** similar `disk-scheduling-raid`, related `scheduler-policies`. **Status:** to be written. **Runner:** gradable today.

#### PAT 13. Seat booking with holds

`iv-ticket-booking` · 90 min · hard · P1 · Tracks: C1

- **New idea:** multi-item changes that are atomic under concurrency.
- **Core:** hold seats, confirm, release, with expiry by fake clock.
- **Follow-up 1:** No double booking under 16 threads. *Test:* final seat ownership checked.
- **Follow-up 2:** All-or-nothing multi-seat holds. *Test:* no partial holds ever observed.
- **Follow-up 3:** Waiting list. *Test:* FIFO order oracle.
- **Trade-offs:** Lock granularity; optimistic versioning. **Concepts:** atomic multi-step update, expiry, concurrency.
- **After this:** you can prevent double booking. **In projects:** reservations and inventory.
- **Required first:** `iv-bank-transfer-concurrent` (CONC 3). **Helpful:** `iv-ttl-kv` (CACHE 2).
- **Cookbook:** related `locks-deadlocks`, related `saga-orchestrator`. **Status:** to be written. **Runner:** gradable today.

#### PAT 14. Limit order book

`iv-order-book` · 120 min · hard · P1 · Tracks: S4 F2

- **New idea:** several indexes over one set of orders, kept consistent.
- **Core:** Add limit orders and match by price then time, returning fills.
- **Follow-up 1:** Cancel and modify in O(log n) or better. *Test:* random streams against a Vec-based matcher.
- **Follow-up 2:** Market, IOC and FOK orders. *Test:* conservation of quantity.
- **Follow-up 3:** Self-trade prevention. *Test:* scripted cases.
- **Trade-offs:** Price levels as BTreeMap of queues; id index. **Concepts:** BTreeMap, VecDeque, index maps, invariants.
- **After this:** you can design matching with fast cancel. **In projects:** exchanges and schedulers.
- **Required first:** `iv-indexed-pq` (COLL 8). **Helpful:** `iv-lru` (CACHE 1).
- **Cookbook:** related `std-btree-binaryheap-ordered`. **Status:** to be written (partly practised by `f2-shrink-a-price-level`). **Runner:** gradable today.

#### PAT 15. Spreadsheet with formulas

`iv-spreadsheet` · 120 min · hard · P1 · Tracks: S4 N

- **New idea:** recalculating only what depends on a change.
- **Core:** set cell to a number or a sum of references; get evaluates.
- **Follow-up 1:** Incremental recalculation of dependents only. *Test:* evaluation counter.
- **Follow-up 2:** Cycle detection that rejects the write. *Test:* state unchanged.
- **Follow-up 3:** Full expressions with ranges. *Test:* random sheets against recompute-all.
- **Trade-offs:** Dirty marking vs eager push. **Concepts:** dependency graph, parsing, incremental computation.
- **After this:** you can build reactive evaluation. **In projects:** spreadsheets and build tools.
- **Required first:** `iv-pratt` (PARSE 12). **Helpful:** `iv-graph-ownership` (OWN 6).
- <details><summary>Links hidden until asked</summary>helpful: `iv-alg-order` (GRAPH 4)</details>
- **Cookbook:** related `pratt-parser`, related `incremental-view-maintenance-dbsp`. **Status:** to be written. **Runner:** gradable today.

### UNSAFE: Unsafe Rust and FFI

Hold the safety obligations yourself. *Core series, 7 parts, about 9 h. Ends in `cap-unsafekit`.* Alongside it: `iv-interleaving-checker` (ATOM 9).

#### UNSAFE 1. Layout-aware binary header codec

`iv-layout-quiz-codec` · 45 min · medium · P3 · Tracks: F2

- **New idea:** memory layout is not a wire format.
- **Core:** Encode and decode a repr(C) header with explicit endianness, no unsafe.
- **Follow-up 1:** Unaligned input. *Test:* every offset.
- **Follow-up 2:** Reject bad enum tags and lengths. *Test:* fuzz-style random bytes, never panics.
- **Follow-up 3:** Zero-copy view variant. *Test:* alignment error path.
- **Trade-offs:** transmute pitfalls; from_le_bytes. **Concepts:** layout, endianness, bounds checks.
- **After this:** you can decode headers without transmute. **In projects:** file and network formats.
- **Required first:** nothing. **Used again in:** `iv-packet-parse` (BIN 6), `iv-png-container` (BIN 13).
- **Cookbook:** similar `repr-layout-ffi`, related `byte-order-keys`. **Status:** to be written (partly practised by `y3-repr-c-header`, `f2-fix-packed-zip-header`). **Runner:** gradable today.

#### UNSAFE 2. A no_std ring buffer and formatter

`iv-nostd-ring` · 60 min · medium · P3 · Tracks: Y2

- **New idea:** code with no heap and no std.
- **Core:** A heapless ring with const capacity using only core.
- **Follow-up 1:** FmtBuf implementing core. *Test:* fmt::Write :: truncation policy tests.
- **Follow-up 2:** parse_u64 with overflow. *Test:* table tests.
- **Follow-up 3:** alloc feature. *Test:* cannot be graded: no feature builds.
- **Trade-offs:** core, alloc, std. **Concepts:** no_std, const generics, MaybeUninit.
- **After this:** you can write embedded-style libraries. **In projects:** firmware and WASM.
- **Required first:** nothing.
- **Cookbook:** similar `cb-no-std`. **Status:** to be written. **Runner:** compiles, but no_std itself is not enforced by the grader.

#### UNSAFE 3. Safe wrapper over a C-style API

`iv-ffi-wrapper` · 60 min · medium · P2 · Tracks: Y3

- **New idea:** a safe wrapper owns a foreign resource.
- **Core:** Wrap an opaque handle with create, use, destroy into a Rust type with Drop and Result.
- **Follow-up 1:** Callbacks with user data, no unwind across the boundary. *Test:* panicking closure becomes an error.
- **Follow-up 2:** Strings in and out with ownership rules. *Test:* interior NUL, free exactly once (counting fake C functions).
- **Follow-up 3:** Send and Sync decision. *Test:* compile-checked use.
- **Trade-offs:** Who frees; repr(C); panic across FFI. **Concepts:** FFI, CString, catch_unwind, Drop.
- **After this:** you can bind C libraries soundly. **In projects:** system integrations.
- **Required first:** `iv-raii-pool` (SMART 1). **Helpful:** `iv-panic-boundary` (ERR 3).
- **Cookbook:** similar\* `ffi-c-interop` (handles, callbacks, panic safety). **Status:** to be written (partly practised by `y3-opaque-handle-api`, `y3-qsort-callback`, `y3-cstring-boundary`). **Runner:** ok (declare the C functions in the same file).

#### UNSAFE 4. Inline vector with MaybeUninit

`iv-fixed-vec` · 90 min · hard · P2 · Tracks: Y2

- **New idea:** uninitialised memory with MaybeUninit.
- **Core:** FixedVec<T, N> with try_push, pop, Deref and Drop.
- **Follow-up 1:** into_iter that drops the rest. *Test:* drop counters.
- **Follow-up 2:** Clone that is panic safe. *Test:* a T whose clone panics half way.
- **Follow-up 3:** try_from_iter. *Test:* partial failure drops everything.
- **Trade-offs:** MaybeUninit arrays; what assume_init promises. **Concepts:** MaybeUninit, panic safety, const generics.
- **After this:** you can manage initialisation by hand. **In projects:** inline collections.
- **Required first:** `iv-raii-pool` (SMART 1). **Used again in:** `iv-myvec` (UNSAFE 5), `iv-pin-mapfuture` (UNSAFE 7), `iv-bump-arena` (MEM 2), `iv-spsc` (ATOM 5).
- **Cookbook:** similar\* `maybeuninit-fixedvec` (builds FixedVec). **Status:** exists: `y2-array-vec`. **Runner:** ok; Miri would be needed.

#### UNSAFE 5. Vec from raw allocation

`iv-myvec` · 120 min · hard · P1 · Tracks: Y2

- **New idea:** allocate, grow and drop raw memory.
- **Core:** push, pop, index, Drop over std::alloc.
- **Follow-up 1:** insert, remove and Deref to slices. *Test:* differential against Vec with drop counting.
- **Follow-up 2:** IntoIter and drain with leak safety. *Test:* forget a drain, nothing is dropped twice.
- **Follow-up 3:** Zero-sized types. *Test:* 1e6 pushes of ().
- **Trade-offs:** Panic safety; growth factor. **Concepts:** raw allocation, NonNull, ptr::read, ZSTs.
- **After this:** you can write a growable container. **In projects:** custom collections.
- **Required first:** `iv-fixed-vec` (UNSAFE 4). **Used again in:** `iv-intrusive-list` (UNSAFE 6).
- **Cookbook:** similar\* `implement-std-from-scratch` (its Q1), related `std-alloc-layout-global-allocator`. **Status:** exists: `y2-mini-vec`. **Runner:** ok; Miri would be needed to check soundness.

#### UNSAFE 6. Intrusive doubly linked list with NonNull

`iv-intrusive-list` · 120 min · hard · P2 · Tracks: Y2

- **New idea:** raw links with aliasing rules.
- **Core:** push and pop at both ends, remove by handle, Drop.
- **Follow-up 1:** Iter and IterMut, double ended. *Test:* model tests.
- **Follow-up 2:** append in O(1). *Test:* structure checks.
- **Follow-up 3:** Cursor that edits mid-list. *Test:* random cursor scripts.
- **Trade-offs:** Aliasing rules; variance and PhantomData. **Concepts:** NonNull, variance, drop check.
- **After this:** you can review unsafe linked code. **In projects:** runtimes and kernels.
- **Required first:** `iv-myvec` (UNSAFE 5). **Helpful:** `iv-linked-list-safe` (OWN 2).
- **Cookbook:** similar\* `nonnull-linked-list` (Deque with iterators), related `phantomdata-typed`. **Status:** exists: `y2-unsafe-linked-list`. **Runner:** ok; Miri would be needed.

#### UNSAFE 7. Pin projection by hand

`iv-pin-mapfuture` · 60 min · hard · P2 · Tracks: C5 Y2

- **New idea:** pinned fields must not move.
- **Core:** A Map future over a pinned inner future.
- **Follow-up 1:** A !Unpin inner future that checks its address. *Test:* moves are detected across polls.
- **Follow-up 2:** Join of two futures. *Test:* either order of completion.
- **Follow-up 3:** State drop in place after Ready. *Test:* drop counters.
- **Trade-offs:** Structural pinning rules; pin-project-lite. **Concepts:** Pin, Unpin, projection, futures.
- **After this:** you can write a future adapter. **In projects:** async libraries.
- **Required first:** `iv-fixed-vec` (UNSAFE 4). **Helpful:** `iv-block-on` (ASYNC 1). **Used again in:** `iv-stream-adapters` (ASYNC 8).
- **Cookbook:** similar\* `pin-projection` (MapFuture). **Status:** to be written. **Runner:** ok; unsoundness itself cannot be detected.

### MEM: Allocators and memory management

Who owns memory, and how it is reused. *Core series, 5 parts, about 8 h. Ends in `cap-interp`.* Alongside it: `iv-alloc-budget` (PERF 2).

#### MEM 1. Slab with a free list

`iv-slab` · 45 min · medium · P1 · Tracks: F3

- **New idea:** a free list threaded through empty slots, with generations.
- **Core:** insert returns a key, get, remove, slots reused.
- **Follow-up 1:** Generational keys. *Test:* stale keys always miss, random ops against a model.
- **Follow-up 2:** iter and retain. *Test:* model comparison.
- **Follow-up 3:** Typed keys per slab. *Test:* compile-checked by use.
- **Trade-offs:** ABA and generations; Vec<Option<T>> cost. **Concepts:** free list, generations, PhantomData.
- **After this:** you can hand out safe handles. **In projects:** ECS, timers and connections.
- **Required first:** nothing. **Helpful:** `iv-graph-ownership` (OWN 6). **Used again in:** `iv-free-list-alloc` (MEM 3), `iv-mark-sweep` (MEM 5), `iv-ordered-map-skiplist` (COLL 14), `iv-treap-order-stats` (COLL 15), `iv-bptree-memory` (COLL 16), `iv-skiplist-spans` (COLL 18), `iv-lru` (CACHE 1), `iv-timing-wheel` (CONC 18), `iv-treiber` (ATOM 8), `iv-heap-file` (STORE 4).
- **Cookbook:** exact `slab-generational` (typed generational handles). **Status:** exists: `f3-generational-slab`. **Runner:** gradable today.

#### MEM 2. Bump allocator

`iv-bump-arena` · 90 min · hard · P1 · Tracks: F3

- **New idea:** allocation by moving a pointer, freeing all at once.
- **Core:** alloc<T>(&self, v) -> &mut T from chunks, with alignment.
- **Follow-up 1:** alloc_slice and alloc_str. *Test:* no overlap over random layouts.
- **Follow-up 2:** Checkpoint and reset. *Test:* memory reused, borrow rules prevent use after reset (positive tests only).
- **Follow-up 3:** Optional drop list. *Test:* destructor counts.
- **Trade-offs:** Why &self can hand out many &mut. **Concepts:** alignment, interior mutability, lifetimes.
- **After this:** you can use arenas for phase data. **In projects:** compilers and request handling.
- **Required first:** `iv-fixed-vec` (UNSAFE 4).
- **Cookbook:** similar\* `bump-arena-allocator` (alignment, checkpoints, drop glue, growth). **Status:** to be written (partly practised by `f3-typed-arena`). **Runner:** ok; Miri would be needed.

#### MEM 3. A free-list allocator over a byte arena

`iv-free-list-alloc` · 120 min · hard · P2 · Tracks: F3

- **New idea:** block headers, splitting and coalescing.
- **Core:** alloc(size) and free(offset) with first fit and block headers, on offsets (no real pointers).
- **Follow-up 1:** Coalesce neighbours. *Test:* full coalescing after freeing everything.
- **Follow-up 2:** Size classes. *Test:* fragmentation bound on a seeded workload.
- **Follow-up 3:** realloc in place when possible. *Test:* content preserved.
- **Trade-offs:** Boundary tags; internal vs external fragmentation. **Concepts:** allocators, intrusive lists in an array.
- **After this:** you can reason about fragmentation. **In projects:** allocators and page managers.
- **Required first:** `iv-slab` (MEM 1). **Used again in:** `iv-buddy` (MEM 4).
- **Cookbook:** similar\* `size-class-allocator` (boundary tags, size classes, realloc). **Status:** to be written (partly practised by `f3-buddy-allocator`). **Runner:** gradable today.

#### MEM 4. Buddy allocator

`iv-buddy` · 90 min · hard · P2 · Tracks: F3

- **New idea:** power-of-two blocks found by bit arithmetic.
- **Core:** alloc and free over 2^k units.
- **Follow-up 1:** Coalescing. *Test:* full merge after all frees.
- **Follow-up 2:** Bad frees detected. *Test:* tables.
- **Follow-up 3:** O(log n) with bitmaps. *Test:* scale.
- **Trade-offs:** Internal fragmentation. **Concepts:** buddy system, bit math.
- **After this:** you can manage page ranges. **In projects:** kernel and GPU allocators.
- **Required first:** `iv-free-list-alloc` (MEM 3). **Helpful:** `iv-alg-bit-tricks` (BIT 3). **Used again in:** `iv-fs-blocks` (OS 5).
- **Cookbook:** related `size-class-allocator`. **Status:** exists: `f3-buddy-allocator`. **Runner:** gradable today.

#### MEM 5. Mark-and-sweep collector on a simulated heap

`iv-mark-sweep` · 120 min · hard · P2 · Tracks: F3

- **New idea:** reachability decides what is alive.
- **Core:** Objects with reference slots, roots, alloc and collect.
- **Follow-up 1:** Iterative marking for deep lists. *Test:* 1e6-long chain.
- **Follow-up 2:** Copying collector behind the same interface. *Test:* reachable graph preserved, oracle compares.
- **Follow-up 3:** Generational with a write barrier. *Test:* remembered-set bugs caught by a random mutator.
- **Trade-offs:** Pause time, throughput, fragmentation. **Concepts:** graph reachability, free lists, barriers.
- **After this:** you can explain tracing collectors. **In projects:** language runtimes.
- **Required first:** `iv-graph-ownership` (OWN 6). **Helpful:** `iv-slab` (MEM 1).
- **Cookbook:** similar\* `garbage-collectors` (these collectors, with a reachability oracle). **Status:** to be written. **Runner:** gradable today.

### COLL: Collections from scratch

The structures every other series reuses. *Core series, 18 parts, about 26 h. Ends in `cap-redis`.* Alongside it: `iv-write-tests` (TEST 2), `iv-shrinker` (TEST 3).

#### COLL 1. Min stack, max queue and sliding-window maximum

`iv-minstack-queue` · 45 min · easy · P1 · Tracks: S5

- **New idea:** an auxiliary structure that answers a running query.
- **Core:** push, pop, min in O(1).
- **Follow-up 1:** Queue from two stacks with max. *Test:* amortised op counts.
- **Follow-up 2:** Sliding window maximum in O(n). *Test:* brute force and scale.
- **Follow-up 3:** Max-frequency stack. *Test:* model.
- **Trade-offs:** Auxiliary stacks; monotonic deques. **Concepts:** monotonic deque, amortisation.
- **After this:** you can keep minima and maxima cheaply. **In projects:** monitoring windows.
- **Required first:** nothing. **Used again in:** `iv-lfu` (CACHE 8).
- <details><summary>Links hidden until asked</summary>used again in: `iv-window-functions` (QUERY 10), `iv-alg-co-window-deque-dp` (COMBO 10)</details>
- **Cookbook:** similar\* `stacks-queues-heaps-questions` (its Q2, Q3 and Q11). **Status:** to be written. **Runner:** gradable today.

#### COLL 2. Growable ring deque

`iv-ring-deque` · 60 min · medium · P2 · Tracks: S5

- **New idea:** wrap-around indices in one buffer.
- **Core:** push and pop at both ends, index, iteration.
- **Follow-up 1:** Growth that relinearises correctly. *Test:* VecDeque differential with wrap.
- **Follow-up 2:** as_slices. *Test:* concatenation equals contents.
- **Follow-up 3:** Overwrite-oldest bounded mode. *Test:* model.
- **Trade-offs:** Masking vs modulo; full vs empty. **Concepts:** ring buffer, MaybeUninit optional.
- **After this:** you can build queues with no shifting. **In projects:** buffers and schedulers.
- **Required first:** nothing. **Used again in:** `iv-binary-heap` (COLL 4), `iv-hit-counter` (CACHE 3), `iv-blocking-queue` (CONC 2), `iv-timing-wheel` (CONC 18), `iv-spsc` (ATOM 5), `iv-length-codec` (BIN 2), `iv-mon-flight-recorder` (MON 3).
- **Cookbook:** similar\* `ring-buffer-zoo` (growable deque and others), related `std-vecdeque-linkedlist-ring-buffers`. **Status:** to be written (partly practised by `s3-ring-buffer`). **Runner:** gradable today.

#### COLL 3. HashMap from scratch

`iv-hashmap-chaining` · 90 min · medium · P1 · Tracks: F4

- **New idea:** hash to a bucket, resize to keep chains short.
- **Core:** insert, get, remove with chaining and resize.
- **Follow-up 1:** Lookup by a borrowed form (String keys, &str queries). *Test:* Borrow-based API tests.
- **Follow-up 2:** Entry-like get_or_insert_with. *Test:* hash-count bound.
- **Follow-up 3:** Robin Hood open addressing behind the same API. *Test:* same differential suite, probe length bound.
- **Trade-offs:** Load factor; HashDoS; tombstones vs backward shift. **Concepts:** hashing, Borrow, resizing.
- **After this:** you can explain every hash map cost. **In projects:** caches and indexes.
- **Required first:** nothing. **Used again in:** `iv-indexed-pq` (COLL 8), `iv-cuckoo` (COLL 13), `iv-bloom` (CACHE 4), `iv-sharded-map` (CONC 10), `iv-extendible-hash` (STORE 12), `iv-hash-agg` (QUERY 4), `iv-inverted-index` (SEARCH 1), `iv-lsh-dedupe` (SEARCH 10), `iv-spatial-grid` (SEARCH 11).
- **Cookbook:** similar\* `implement-std-from-scratch` (its Q6: chaining, resize, borrowed lookup), similar\* `open-addressing-map` (Robin Hood with backward shift). **Status:** to be written (partly practised by `f4-open-addressing-map`, `s4-open-addressing-map`). **Runner:** gradable today.

#### COLL 4. Priority queue: a binary heap

`iv-binary-heap` · 60 min · medium · P1 · Tracks: S5

- **New idea:** a tree in an array kept in heap order.
- **Core:** A max-priority queue without BinaryHeap: push, pop, peek, len.
- **Follow-up 1:** Build from a Vec in O(n). *Test:* comparison-counting items bound the work.
- **Follow-up 2:** Pluggable ordering (comparator or Reverse). *Test:* min and max from one type.
- **Follow-up 3:** Keep only the k largest of a stream in O(k) space. *Test:* oracle by sorting.
- **Trade-offs:** Sift with swaps vs a hole; binary vs d-ary; stability. **Concepts:** binary heap, sift, generics, Ord.
- **After this:** you can always get the most urgent item. **In projects:** schedulers and top-k.
- **Required first:** nothing. **Helpful:** `iv-ring-deque` (COLL 2). **Used again in:** `iv-median-stream` (COLL 5), `iv-indexed-pq` (COLL 8), `iv-leaderboard` (COLL 11), `iv-range-queries` (COLL 12), `iv-ttl-kv` (CACHE 2), `iv-count-min` (CACHE 5), `iv-autocomplete` (SEARCH 6).
- <details><summary>Links hidden until asked</summary>used again in: `iv-splitwise` (PAT 7), `iv-pipeline-stages` (CONC 14), `iv-job-scheduler` (CONC 15), `iv-huffman` (BIN 9), `iv-compaction` (STORE 6), `iv-external-sort` (QUERY 3), `iv-bm25` (SEARCH 5), `iv-knn` (SEARCH 9), `iv-alg-routes` (GRAPH 5), `iv-alg-gr-deadlines` (GRD 2), `iv-alg-gr-huffman` (GRD 3), `iv-alg-co-greedy-heap` (COMBO 3)</details>
- **Cookbook:** exact `stacks-queues-heaps-questions` (its Q23: generic heap with O(n) heapify), similar\* `heap-patterns` (d-ary heap, part), related `std-btree-binaryheap-ordered`. **Status:** staged draft `s5-binary-heap` (new in build-your-own). **Runner:** gradable today.

#### COLL 5. Running median and sliding median

`iv-median-stream` · 60 min · medium · P2 · Tracks: S5

- **New idea:** two heaps that balance each other.
- **Core:** add and median over a stream.
- **Follow-up 1:** Remove arbitrary values. *Test:* oracle by sorting.
- **Follow-up 2:** Sliding-window median. *Test:* brute force and scale.
- **Follow-up 3:** Approximate quantiles in bounded memory. *Test:* error bound.
- **Trade-offs:** Two heaps with lazy deletion vs ordered multiset. **Concepts:** two heaps, lazy deletion.
- **After this:** you can track medians online. **In projects:** latency dashboards.
- **Required first:** `iv-binary-heap` (COLL 4). **Used again in:** `iv-mon-anomaly` (MON 8).
- **Cookbook:** similar\* `stacks-queues-heaps-questions` (its Q17). **Status:** to be written. **Runner:** gradable today.

#### COLL 6. Trie with prefix queries

`iv-trie` · 60 min · medium · P1 · Tracks: F4

- **New idea:** keys share paths by prefix.
- **Core:** insert, contains, count_with_prefix, remove.
- **Follow-up 1:** List words under a prefix in order, with a limit. *Test:* BTreeSet oracle.
- **Follow-up 2:** Longest stored prefix of a text. *Test:* oracle scan.
- **Follow-up 3:** remove prunes dead nodes. *Test:* node count equals a freshly built trie.
- **Trade-offs:** Children as array, map or sorted Vec; arena vs Box; bytes vs chars. **Concepts:** trie, arena, pruning.
- **After this:** you can answer prefix queries. **In projects:** routers and autocomplete.
- **Required first:** nothing. **Helpful:** `iv-graph-ownership` (OWN 6). **Used again in:** `iv-file-system-tree` (PAT 11), `iv-pubsub` (CONC 12), `iv-router` (WEB 1), `iv-autocomplete` (SEARCH 6), `iv-fuzzy` (SEARCH 7), `iv-aho-corasick` (SEARCH 8), `iv-mon-flame` (MON 7).
- **Cookbook:** exact `trie-radix` (insert, remove with pruning, prefix listing, longest prefix), exact `tries-core-questions` (its Q1: prefix counts and delete). **Status:** staged draft `f4-prefix-trie` (new in build-your-own). **Runner:** gradable today.

#### COLL 7. Union-find

`iv-union-find` · 60 min · medium · P1 · Tracks: F4

- **New idea:** merge groups and ask who is together.
- **Core:** union, connected, component count in near-constant time.
- **Follow-up 1:** Component size and the largest component. *Test:* oracle by relabelling.
- **Follow-up 2:** Grow with new elements and arbitrary string keys. *Test:* model tests.
- **Follow-up 3:** Rollback to a checkpoint. *Test:* random union/rollback scripts against replay.
- **Trade-offs:** Path compression vs rollback; union by size or rank. **Concepts:** disjoint sets, amortised analysis.
- **After this:** you can track connectivity as links appear. **In projects:** clustering and deduplication.
- **Required first:** nothing.
- <details><summary>Links hidden until asked</summary>used again in: `iv-hindley-milner` (LANG 9), `iv-unification` (LANG 10), `iv-lsh-dedupe` (SEARCH 10), `iv-alg-co-offline-dsu` (COMBO 9)</details>
- **Cookbook:** similar `union-find` (no component size query), similar `graphs-and-union-find-questions`, similar `amortized-analysis-online-algorithms`. **Status:** staged draft `f4-union-find` (new in build-your-own). **Runner:** gradable today.

#### COLL 8. Priority queue with change-priority

`iv-indexed-pq` · 90 min · hard · P1 · Tracks: S5

- **New idea:** a map from item to position makes updates possible.
- **Core:** push(id, priority), pop, change_priority, priority_of, in O(log n).
- **Follow-up 1:** remove(id) from anywhere. *Test:* random ops against a map oracle.
- **Follow-up 2:** Ties served in insertion order. *Test:* stability tests.
- **Follow-up 3:** Use it to drive shortest paths. *Test:* distances equal an O(V*E) oracle.
- **Trade-offs:** Lazy deletion vs a position map. **Concepts:** indexed heap, position map, invariants.
- **After this:** you can change urgency of queued work. **In projects:** schedulers and routing.
- **Required first:** `iv-binary-heap` (COLL 4). **Helpful:** `iv-hashmap-chaining` (COLL 3). **Used again in:** `iv-order-book` (PAT 14), `iv-job-scheduler` (CONC 15).
- <details><summary>Links hidden until asked</summary>used again in: `iv-lru-k` (STORE 8), `iv-hnsw` (SEARCH 14)</details>
- **Cookbook:** similar `heap-patterns` (IndexedHeap lowers a key only, no remove), similar `priority-queue-zoo`, similar `stacks-queues-heaps-questions`. **Status:** staged draft `s5-indexed-priority-queue` (new in build-your-own). **Runner:** gradable today.

#### COLL 9. Bitset and compressed bitmap

`iv-bitset-roaring` · 90 min · medium · P2 · Tracks: F5

- **New idea:** one bit per member, containers chosen by density.
- **Core:** A growable bitset with set ops and iteration.
- **Follow-up 1:** rank and select. *Test:* scan oracle.
- **Follow-up 2:** Roaring-style array and bitmap containers. *Test:* HashSet oracle and size bound.
- **Follow-up 3:** Run containers. *Test:* conversion invariants.
- **Trade-offs:** Density thresholds. **Concepts:** bit operations, compression.
- **After this:** you can do fast set algebra. **In projects:** analytics filters.
- **Required first:** nothing. **Helpful:** `iv-alg-bit-tricks` (BIT 3). **Used again in:** `iv-bitmap-index` (STORE 11).
- **Cookbook:** similar\* `bitset-rank-select-roaring`. **Status:** to be written (partly practised by `f5-bitset-iter`). **Runner:** gradable today.

#### COLL 10. Calendar booking

`iv-interval-booking` · 45 min · medium · P1 · Tracks: S4

- **New idea:** neighbours in an ordered map detect overlap.
- **Core:** book(start, end) refusing overlaps.
- **Follow-up 1:** Allow up to k overlaps. *Test:* sweep oracle.
- **Follow-up 2:** Free slots across many calendars. *Test:* oracle.
- **Follow-up 3:** Cancel and reschedule. *Test:* model.
- **Trade-offs:** Ordered map neighbours vs interval tree. **Concepts:** BTreeMap range, sweep line.
- **After this:** you can book without double use. **In projects:** calendars and range allocators.
- **Required first:** nothing. **Used again in:** `iv-tcp-reassembly` (NET 2), `iv-versioned-map` (TXN 1), `iv-consistent-hash` (DIST 8), `iv-alg-iv-merge` (IVL 1).
- **Cookbook:** similar `greedy-and-interval-algorithms` (MyCalendar variants), related `intervals-greedy-bit-questions`. **Status:** to be written (partly practised by `s4-interval-map`). **Runner:** gradable today.

#### COLL 11. Leaderboard with ranks

`iv-leaderboard` · 60 min · medium · P1 · Tracks: S4

- **New idea:** ordering and rank need different indexes.
- **Core:** update score, top k, rank of a player.
- **Follow-up 1:** Ties by time of reaching the score. *Test:* oracle by sort.
- **Follow-up 2:** Window around a player. *Test:* oracle.
- **Follow-up 3:** Rank in O(log n). *Test:* scale test (needs a Fenwick or augmented tree).
- **Trade-offs:** BTreeSet cannot rank in O(log n). **Concepts:** BTreeSet, order statistics, indexing.
- **After this:** you can serve ranks at scale. **In projects:** games and quotas.
- **Required first:** `iv-binary-heap` (COLL 4). **Helpful:** `iv-range-queries` (COLL 12), `iv-treap-order-stats` (COLL 15).
- **Cookbook:** similar `std-btree-binaryheap-ordered`, similar `mini-redis` (sorted set with ranks). **Status:** to be written. **Runner:** gradable today.

#### COLL 12. Fenwick tree, then segment tree

`iv-range-queries` · 120 min · hard · P1 · Tracks: F4

- **New idea:** partial aggregates stored in a tree.
- **Core:** Point add and range sum in O(log n).
- **Follow-up 1:** Order statistic: smallest index whose prefix reaches k. *Test:* linear-scan oracle.
- **Follow-up 2:** A segment tree over any associative operation, including non-commutative. *Test:* string concatenation oracle.
- **Follow-up 3:** Range add with range sum (lazy propagation). *Test:* Vec oracle.
- **Trade-offs:** Fenwick vs segment tree; iterative vs recursive; laziness. **Concepts:** Fenwick, segment tree, monoid, lazy propagation.
- **After this:** you can answer range questions under updates. **In projects:** time buckets and ranks.
- **Required first:** nothing. **Helpful:** `iv-binary-heap` (COLL 4). **Used again in:** `iv-leaderboard` (COLL 11), `iv-treap-order-stats` (COLL 15).
- <details><summary>Links hidden until asked</summary>used again in: `iv-window-functions` (QUERY 10)</details>
- **Cookbook:** exact `segment-fenwick` (Fenwick with k-th, monoid tree, lazy range add), related `amortized-analysis-online-algorithms`. **Status:** staged draft `f4-range-queries` (new in build-your-own). **Runner:** gradable today.

#### COLL 13. Cuckoo hash map and filter

`iv-cuckoo` · 120 min · hard · P3 · Tracks: F4

- **New idea:** two homes per key and eviction chains.
- **Core:** Two-table cuckoo map with relocation and rehash.
- **Follow-up 1:** Stash and bounded kicks. *Test:* adversarial seeds.
- **Follow-up 2:** Cuckoo filter with delete. *Test:* false-positive rate bound.
- **Follow-up 3:** Load factor study. *Test:* printed, not asserted.
- **Trade-offs:** Worst-case O(1) lookups; insert failures. **Concepts:** cuckoo hashing, fingerprints.
- **After this:** you can get constant worst-case lookups. **In projects:** network tables.
- **Required first:** `iv-hashmap-chaining` (COLL 3).
- **Cookbook:** similar\* `cuckoo-hashing` (both). **Status:** to be written. **Runner:** gradable today.

#### COLL 14. Ordered map as a skip list

`iv-ordered-map-skiplist` · 120 min · hard · P2 · Tracks: S4

- **New idea:** randomised levels keep an ordered list fast.
- **Core:** insert, get, remove, in-order iteration with an arena.
- **Follow-up 1:** Range queries. *Test:* BTreeMap oracle.
- **Follow-up 2:** Rank and select with spans. *Test:* sorted Vec oracle.
- **Follow-up 3:** Deterministic levels from a seed. *Test:* structure checks.
- **Trade-offs:** Skip list vs B-tree vs treap. **Concepts:** skip list, arena, probabilistic balance.
- **After this:** you can build an ordered map. **In projects:** memtables.
- **Required first:** `iv-slab` (MEM 1). **Used again in:** `iv-bptree-memory` (COLL 16), `iv-skiplist-spans` (COLL 18), `iv-memtable-flush` (STORE 5).
- **Cookbook:** similar\* `skiplist` (arena skip list), similar `mini-redis` (span-based ranks). **Status:** to be written. **Runner:** gradable today.

#### COLL 15. Treap with order statistics and intervals

`iv-treap-order-stats` · 120 min · hard · P3 · Tracks: S4

- **New idea:** subtree summaries kept through rotations or splits.
- **Core:** split and merge treap: insert, remove, k-th, rank.
- **Follow-up 1:** Range reverse or range sum with augmentation. *Test:* Vec oracle.
- **Follow-up 2:** Interval overlap queries. *Test:* scan oracle.
- **Follow-up 3:** Persistent version. *Test:* old roots unchanged.
- **Trade-offs:** Randomised balance; augmentation. **Concepts:** treap, augmentation.
- **After this:** you can add rank and range queries to a tree. **In projects:** leaderboards.
- **Required first:** `iv-slab` (MEM 1). **Helpful:** `iv-range-queries` (COLL 12). **Used again in:** `iv-leaderboard` (COLL 11), `iv-crdt-text-fugue` (CRDT 12), `iv-alg-iv-queries` (IVL 4).
- **Cookbook:** similar\* `treap-interval` (order statistics and intervals). **Status:** to be written (partly practised by `s4-interval-map`). **Runner:** gradable today.

#### COLL 16. In-memory B+ tree

`iv-bptree-memory` · 120 min · hard · P1 · Tracks: S4

- **New idea:** wide nodes that split and stay balanced.
- **Core:** insert and get with node splits, order as a parameter.
- **Follow-up 1:** Range iteration over the leaf chain. *Test:* BTreeMap oracle.
- **Follow-up 2:** Delete with borrow and merge. *Test:* invariant checker after every op.
- **Follow-up 3:** Bulk load. *Test:* occupancy bounds.
- **Trade-offs:** Fan-out vs cache lines; arena nodes. **Concepts:** B+ tree, splits, invariants.
- **After this:** you can explain database indexes. **In projects:** storage engines.
- **Required first:** `iv-slab` (MEM 1). **Helpful:** `iv-ordered-map-skiplist` (COLL 14). **Used again in:** `iv-disk-bptree` (STORE 13), `iv-latch-crabbing` (STORE 15).
- **Cookbook:** similar\* `bplus-tree` (all four parts), similar `btree-bulk-load-prefix-compression`. **Status:** to be written. **Runner:** gradable today.

#### COLL 17. Bounding volume hierarchy in a flat array

`iv-bvh-flat` · 120 min · hard · P2 · prep · Tracks: COLL

- **New idea:** a tree of bounding boxes lets a search skip whole groups.
- **Core:** Build a median-split BVH over boxes into a depth-first Vec and answer nearest-hit queries with an explicit stack.
- **Follow-up 1:** near-child-first and pruning by best hit. *Test:* count visited nodes deterministically and compare with the unordered version ; any-hit query :: agrees with brute force on seeded scenes.
- **Required first:** `iv-ray-intersection` (NUM 3), `iv-split-borrow-bank` (OWN 1).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

#### COLL 18. Rank queries in a skip list with link spans

`iv-skiplist-spans` · 90 min · hard · P2 · prep · Tracks: COLL

- **New idea:** counts stored on links give rank and select.
- **Core:** Add a span to every forward link of a given skip list, keep it right through insert and delete, and implement rank and get-by-rank.
- **Follow-up 1:** structural invariant. *Test:* after seeded random operations every span equals the level-0 distance ; rank and select agree with a model :: differential test against a sorted Vec.
- **Required first:** `iv-ordered-map-skiplist` (COLL 14), `iv-slab` (MEM 1).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

### CACHE: Caches and bounded memory

Keep the useful data when memory is limited, or approximate it. *Core series, 10 parts, about 10 h. Ends in `cap-redis`.* Alongside it: `iv-mon-metrics` (MON 2).

#### CACHE 1. LRU cache

`iv-lru` · 60 min · medium · P1 · Tracks: F4

- **New idea:** recency order kept beside a key map.
- **Core:** get and put in O(1) with a capacity.
- **Follow-up 1:** remove, peek and eviction reporting. *Test:* Vec model.
- **Follow-up 2:** Weight-based capacity. *Test:* total weight invariant.
- **Follow-up 3:** Thread-safe sharded version. *Test:* per-shard invariants under 8 threads.
- **Trade-offs:** HashMap plus list of indices; strict LRU vs CLOCK. **Concepts:** index-linked list, HashMap, sharding.
- **After this:** you can bound any cache. **In projects:** HTTP, DNS and page caches.
- **Required first:** nothing. **Helpful:** `iv-slab` (MEM 1), `iv-linked-list-safe` (OWN 2). **Used again in:** `iv-order-book` (PAT 14), `iv-ttl-kv` (CACHE 2), `iv-lfu` (CACHE 8), `iv-generation-cache` (CACHE 9), `iv-page-replacement` (OS 2), `iv-http-cache` (WEB 5), `iv-lru-k` (STORE 8), `iv-buffer-pool` (STORE 9).
- **Cookbook:** similar\* `lru-cache` (get and put on an index-linked list), similar `concurrency-interview-questions` (its Q3: ordered-map LRU and sharded version), similar `cache-policy-zoo`. **Status:** staged draft: `f4-lru-cache-arena`. **Runner:** gradable today.

#### CACHE 2. In-memory key-value store with TTL

`iv-ttl-kv` · 60 min · medium · P1 · Tracks: S4

- **New idea:** entries expire against an injected clock.
- **Core:** set, get, delete with per-key expiry and an injected clock.
- **Follow-up 1:** Active expiry that is not a full scan. *Test:* work counter stays proportional to expired keys.
- **Follow-up 2:** Max size with eviction. *Test:* model with fake clock.
- **Follow-up 3:** Transactions begin, commit, rollback, nested. *Test:* random scripts against a stack-of-maps model.
- **Trade-offs:** Lazy vs active expiry; heap vs wheel. **Concepts:** fake clock, lazy deletion, BinaryHeap or BTreeMap.
- **After this:** you can add expiry to stored state. **In projects:** sessions and Redis-like stores.
- **Required first:** `iv-lru` (CACHE 1). **Helpful:** `iv-binary-heap` (COLL 4). **Used again in:** `iv-ticket-booking` (PAT 13), `iv-sampled-expiry` (CACHE 10), `iv-http-cache` (WEB 5), `iv-sessions` (SEC 5), `iv-sec-passwords` (SEC 8), `iv-rate-limiter` (RESIL 2), `iv-idempotency-store` (RESIL 4), `iv-service-registry` (RESIL 6), `iv-lease-fencing` (DIST 7).
- **Cookbook:** similar `mini-redis` (expiry and transactions), similar `resp-protocol` (a store with a fake clock), related `timing-wheel`. **Status:** to be written. **Runner:** gradable today.

#### CACHE 3. Hit counter and sliding statistics

`iv-hit-counter` · 45 min · easy · P1 · Tracks: S5

- **New idea:** time buckets in a ring.
- **Core:** hit(t), count over the last 300 s, fake clock.
- **Follow-up 1:** Bounded memory with buckets. *Test:* error bound against an exact log.
- **Follow-up 2:** Out-of-order timestamps within a window. *Test:* oracle.
- **Follow-up 3:** Per-key counts with idle eviction. *Test:* map size bound.
- **Trade-offs:** Exact log vs buckets. **Concepts:** ring buffer, time buckets.
- **After this:** you can count recent events in fixed memory. **In projects:** quotas and dashboards.
- **Required first:** `iv-ring-deque` (COLL 2). **Used again in:** `iv-job-scheduler` (CONC 15), `iv-rate-limiter` (RESIL 2), `iv-circuit-breaker` (RESIL 3), `iv-mon-slo` (MON 5).
- **Cookbook:** similar `ring-buffer-zoo` (window statistics), related `concurrent-counters-and-statistics`. **Status:** to be written. **Runner:** gradable today.

#### CACHE 4. Bloom filter

`iv-bloom` · 45 min · medium · P2 · Tracks: F4

- **New idea:** a set that can say definitely not.
- **Core:** A Bloom filter sized from an expected count and a false-positive target.
- **Follow-up 1:** Union of two filters. *Test:* equals inserting both sets.
- **Follow-up 2:** Counting variant with delete. *Test:* no false negatives after deletes.
- **Follow-up 3:** Serialise and load. *Test:* round trip.
- **Trade-offs:** Memory vs error rate; double hashing. **Concepts:** bloom filter, hashing, error bounds.
- **After this:** you can skip expensive lookups. **In projects:** storage engines.
- **Required first:** `iv-hashmap-chaining` (COLL 3). **Helpful:** `iv-alg-bit-flags` (BIT 1). **Used again in:** `iv-count-min` (CACHE 5), `iv-hll` (CACHE 6), `iv-bloom-sstable` (STORE 7), `iv-hash-join` (QUERY 5).
- **Cookbook:** exact `bloom-filter` (insert and query; no sizing from a target rate). **Status:** exists: `f4-bloom-filter`. **Runner:** gradable today.

#### CACHE 5. Heavy hitters in fixed memory

`iv-count-min` · 60 min · medium · P2 · Tracks: F4

- **New idea:** counts that only over-estimate.
- **Core:** A Count-Min sketch with point queries.
- **Follow-up 1:** Never underestimates and meets its error bound. *Test:* Zipf streams against exact counts.
- **Follow-up 2:** Top-k heavy hitters. *Test:* recall bound.
- **Follow-up 3:** Merge two sketches. *Test:* equals sketching both streams.
- **Trade-offs:** Width vs depth. **Concepts:** count-min sketch, streaming.
- **After this:** you can find hot keys. **In projects:** abuse detection.
- **Required first:** `iv-bloom` (CACHE 4). **Helpful:** `iv-binary-heap` (COLL 4). **Used again in:** `iv-cardinality` (QUERY 12).
- **Cookbook:** similar\* `count-min-sketch`. **Status:** exists: `f4-count-min-sketch`. **Runner:** gradable today.

#### CACHE 6. Count distinct values in fixed memory

`iv-hll` · 60 min · medium · P3 · Tracks: F4

- **New idea:** estimate distinct counts from hash patterns.
- **Core:** HyperLogLog add and estimate.
- **Follow-up 1:** Relative error bound over seeds. *Test:* exact set oracle.
- **Follow-up 2:** Merge. *Test:* equals one sketch of the union.
- **Follow-up 3:** Small-range correction. *Test:* low cardinalities.
- **Trade-offs:** Register count. **Concepts:** hyperloglog, estimation.
- **After this:** you can count uniques cheaply. **In projects:** analytics.
- **Required first:** `iv-bloom` (CACHE 4).
- **Cookbook:** similar\* `hyperloglog`. **Status:** to be written. **Runner:** gradable today.

#### CACHE 7. Sampling from streams

`iv-reservoir` · 45 min · medium · P3 · Tracks: S3

- **New idea:** a uniform sample from an unknown length.
- **Core:** Reservoir sample of k items.
- **Follow-up 1:** Weighted reservoir. *Test:* frequency bounds over seeds.
- **Follow-up 2:** Fisher-Yates and the biased variant. *Test:* chi-square detects the bias.
- **Follow-up 3:** Alias method. *Test:* distribution test.
- **Trade-offs:** Proofs of uniformity. **Concepts:** randomised algorithms, statistics in tests.
- **After this:** you can sample logs and traces. **In projects:** telemetry.
- **Required first:** nothing. **Used again in:** `iv-cardinality` (QUERY 12).
- **Cookbook:** similar\* `sampling` (reservoir, weighted, shuffles). **Status:** to be written. **Runner:** gradable today.

#### CACHE 8. LFU cache in O(1)

`iv-lfu` · 90 min · hard · P1 · Tracks: F4

- **New idea:** eviction by frequency in constant time.
- **Core:** get and put with least-frequently-used eviction, ties by recency.
- **Follow-up 1:** Strict O(1). *Test:* scale test.
- **Follow-up 2:** Frequency ageing. *Test:* scripted traces.
- **Follow-up 3:** A CachePolicy trait shared with LRU. *Test:* one suite runs both.
- **Trade-offs:** min_freq maintenance; scan resistance. **Concepts:** frequency buckets, index-linked lists.
- **After this:** you can choose a policy for a workload. **In projects:** CDN and buffer caches.
- **Required first:** `iv-lru` (CACHE 1). **Helpful:** `iv-minstack-queue` (COLL 1).
- **Cookbook:** exact `combo-algorithms` (O(1) LFU with a naive oracle), similar `cache-policy-zoo`. **Status:** to be written. **Runner:** gradable today.

#### CACHE 9. Generation-stamped cache invalidation

`iv-generation-cache` · 90 min · hard · P2 · prep · Tracks: CACHE

- **New idea:** a version stamp checked at install time stops a slow reader from caching a stale answer.
- **Core:** Wrap a changing store and a query cache so writes bump a generation and results computed from an older one are never installed, with the race driven by explicit hooks.
- **Follow-up 1:** Drop only the keys a changed item can affect. *Test:* random writes against uncached recomputation plus surviving-hit counts ; Record the generation of every concurrent answer and replay the write log :: each answer equals the model at that generation, with no timing assertions.
- **Required first:** `iv-lru` (CACHE 1), `iv-rcu-snapshot` (ATOM 3).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

#### CACHE 10. Expire keys nobody reads by random sampling

`iv-sampled-expiry` · 60 min · medium · P2 · prep · Tracks: CACHE

- **New idea:** random sampling reclaims dead entries at bounded cost.
- **Core:** Build an expiry index with O(1) set, remove and random pick (Vec, swap_remove, position map) and a seeded xorshift, then a cycle that samples 20 entries and repeats while more than 25 percent were expired, with a round limit.
- **Follow-up 1:** consistency under random set and remove. *Test:* seeded differential test against a HashMap ; garbage reduction :: fixed seed and manual time, one cycle lowers a 75 percent expired set below half and never removes a live key.
- **Required first:** `iv-ttl-kv` (CACHE 2).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

### CONC: Concurrency: locks to a job system

From one shared queue to schedulers and pipelines. *Core series, 22 parts, about 24 h. Ends in `cap-jobsched`.* Alongside it: `iv-mon-lockdep` (MON 6), `iv-mon-metrics` (MON 2).

#### CONC 1. Turn-taking puzzles

`iv-print-in-order` · 45 min · easy · P2 · Tracks: C1

- **New idea:** threads wait for their turn.
- **Core:** Print FooBar and zero-even-odd in order with n threads.
- **Follow-up 1:** Same with channels only. *Test:* identical output.
- **Follow-up 2:** With thread parking. *Test:* identical output.
- **Follow-up 3:** Generalise to k threads in a ring. *Test:* output oracle.
- **Trade-offs:** Signalling primitives compared. **Concepts:** Condvar, channels, park.
- **After this:** you can signal between threads. **In projects:** staged startup.
- **Required first:** nothing.
- **Cookbook:** similar\* `concurrency-interview-questions` (its Q6 and Q7). **Status:** to be written. **Runner:** gradable today.

#### CONC 2. Bounded blocking queue

`iv-blocking-queue` · 60 min · medium · P1 · Tracks: C1

- **New idea:** wait on a condition in a loop, and close cleanly.
- **Core:** push blocks when full, pop when empty.
- **Follow-up 1:** close() that wakes everyone and drains. *Test:* producers get their values back.
- **Follow-up 2:** try and timeout variants. *Test:* generous upper bounds only.
- **Follow-up 3:** Batch pop. *Test:* no loss or duplication across 8 threads.
- **Trade-offs:** Two condvars; spurious wakeups; notify_one hazards. **Concepts:** Mutex, Condvar, shutdown.
- **After this:** you can hand work between threads. **In projects:** every worker system.
- **Required first:** nothing. **Helpful:** `iv-ring-deque` (COLL 2). **Used again in:** `iv-logger` (PAT 9), `iv-semaphore` (CONC 5), `iv-barrier-latch` (CONC 6), `iv-channel-mpsc` (CONC 7), `iv-oneshot-thread` (CONC 8), `iv-singleflight` (CONC 11), `iv-park-wake-handshake` (CONC 22), `iv-spinlock` (ATOM 1), `iv-block-on` (ASYNC 1), `iv-process-pipeline` (IO 7).
- **Cookbook:** similar\* `concurrency-interview-questions` (its Q1), similar `mutex-condvar-channel`. **Status:** exists: `c1-blocking-queue`. **Runner:** gradable today.

#### CONC 3. Concurrent bank with no deadlock

`iv-bank-transfer-concurrent` · 60 min · medium · P1 · Tracks: C1

- **New idea:** a global lock order prevents deadlock.
- **Core:** transfer between accounts each behind its own lock.
- **Follow-up 1:** Consistent total() while transfers run. *Test:* invariant sampled by a reader.
- **Follow-up 2:** try-lock with backoff alternative. *Test:* completes under stress.
- **Follow-up 3:** Lock-order checker that reports potential deadlock. *Test:* scripted acquisition orders.
- **Trade-offs:** Lock ordering vs global lock. **Concepts:** lock ordering, deadlock.
- **After this:** you can lock several objects safely. **In projects:** ledgers and inventories.
- **Required first:** `iv-split-borrow-bank` (OWN 1). **Used again in:** `iv-ticket-booking` (PAT 13), `iv-dining` (CONC 4), `iv-lock-manager` (TXN 4), `iv-mon-lockdep` (MON 6).
- **Cookbook:** similar\* `locks-deadlocks` (lock ordering by id), similar\* `concurrency-interview-questions` (its Q11: lock-order graph). **Status:** staged draft: `c1-fix-transfer-deadlock`. **Runner:** gradable today.

#### CONC 4. Dining philosophers

`iv-dining` · 45 min · easy · P3 · Tracks: C1

- **New idea:** the conditions that make deadlock possible.
- **Core:** n philosophers eat m times without deadlock.
- **Follow-up 1:** Waiter and ordering solutions behind one trait. *Test:* both finish.
- **Follow-up 2:** No starvation. *Test:* per-philosopher meal bound.
- **Follow-up 3:** Detect the deadlocking version with try_lock timeouts. *Test:* bounded run.
- **Trade-offs:** Coffman conditions. **Concepts:** deadlock avoidance.
- **After this:** you can break a deadlock four ways. **In projects:** resource allocation.
- **Required first:** `iv-bank-transfer-concurrent` (CONC 3).
- **Cookbook:** similar\* `concurrency-interview-questions` (its Q5). **Status:** to be written. **Runner:** gradable today.

#### CONC 5. Counting and weighted semaphore

`iv-semaphore` · 60 min · medium · P1 · Tracks: C1

- **New idea:** permits returned by a guard, handed out fairly.
- **Core:** acquire and release through an RAII permit.
- **Follow-up 1:** Weighted acquire without starvation (FIFO). *Test:* arrival order asserted with state probes.
- **Follow-up 2:** try_acquire and close. *Test:* waiters get an error.
- **Follow-up 3:** Async version with wakers. *Test:* manual polling.
- **Trade-offs:** Fairness costs throughput. **Concepts:** Condvar, RAII, fairness.
- **After this:** you can limit concurrency. **In projects:** pools and rate gates.
- **Required first:** `iv-blocking-queue` (CONC 2). **Helpful:** `iv-raii-pool` (SMART 1). **Used again in:** `iv-rwlock` (CONC 17), `iv-fanout-limit` (ASYNC 5), `iv-async-primitives` (ASYNC 10), `iv-sbx-quota` (SBX 3), `iv-aimd-concurrency` (RESIL 12).
- **Cookbook:** exact `concurrency-interview-questions` (its Q9: unfair single permit). **Status:** exists: `c1-fair-semaphore`. **Runner:** gradable today.

#### CONC 6. Barrier, latch and phaser

`iv-barrier-latch` · 60 min · medium · P2 · Tracks: C1

- **New idea:** generations make a meeting point reusable.
- **Core:** A reusable barrier for n threads that reports a leader.
- **Follow-up 1:** Countdown latch. *Test:* ordering checks over many rounds.
- **Follow-up 2:** Dynamic registration. *Test:* join and leave between phases.
- **Follow-up 3:** Abort wakes all with an error. *Test:* no hang.
- **Trade-offs:** Generation counters. **Concepts:** Condvar, generations.
- **After this:** you can coordinate phases. **In projects:** parallel jobs.
- **Required first:** `iv-blocking-queue` (CONC 2).
- **Cookbook:** exact `concurrency-interview-questions` (its Q8: reusable barrier). **Status:** to be written. **Runner:** gradable today.

#### CONC 7. Build an MPSC channel

`iv-channel-mpsc` · 60 min · medium · P1 · Tracks: C2

- **New idea:** disconnection is part of the protocol.
- **Core:** unbounded send and blocking recv.
- **Follow-up 1:** Disconnect on both sides. *Test:* errors in the right order.
- **Follow-up 2:** Bounded with backpressure. *Test:* capacity never exceeded.
- **Follow-up 3:** select over two channels by polling a shared condvar. *Test:* no lost message.
- **Trade-offs:** Sender counts; wakeups. **Concepts:** channels, disconnect, Arc.
- **After this:** you can design channels that end cleanly. **In projects:** pipelines.
- **Required first:** `iv-blocking-queue` (CONC 2). **Used again in:** `iv-thread-pool` (CONC 9), `iv-pubsub` (CONC 12), `iv-pipeline-stages` (CONC 14), `iv-async-channel` (ASYNC 3).
- **Cookbook:** similar `mutex-condvar-channel` (bounded MPMC built from pieces). **Status:** exists: `c2-bounded-channel`. **Runner:** gradable today.

#### CONC 8. Oneshot and future-like handle between threads

`iv-oneshot-thread` · 45 min · medium · P3 · Tracks: C2

- **New idea:** a slot filled once that one waiter takes.
- **Core:** send once, recv blocks.
- **Follow-up 1:** Dropped sender wakes with an error. *Test:* scripted.
- **Follow-up 2:** Combinator wait_all. *Test:* order preserved.
- **Follow-up 3:** The async version. *Test:* manual polling.
- **Trade-offs:** State machine under one lock. **Concepts:** state enum, Condvar, Waker.
- **After this:** you can return results from workers. **In projects:** RPC clients.
- **Required first:** `iv-blocking-queue` (CONC 2). **Used again in:** `iv-thread-pool` (CONC 9), `iv-executor` (ASYNC 2).
- **Cookbook:** related `rust-async-qa`. **Status:** to be written (partly practised by `c5-oneshot-channel`). **Runner:** gradable today.

#### CONC 9. Thread pool

`iv-thread-pool` · 60 min · medium · P1 · Tracks: C2

- **New idea:** jobs are boxed closures run by fixed workers.
- **Core:** execute(closure) on n workers, graceful shutdown on drop.
- **Follow-up 1:** submit returning a handle with the result. *Test:* panics become errors and workers survive.
- **Follow-up 2:** Bounded queue with rejection. *Test:* counts add up.
- **Follow-up 3:** wait_idle. *Test:* no early return across many rounds.
- **Trade-offs:** Shared receiver vs per-worker queues. **Concepts:** threads, channels, Box<dyn FnOnce>, panic isolation.
- **After this:** you can run work without unbounded threads. **In projects:** servers.
- **Required first:** `iv-channel-mpsc` (CONC 7). **Helpful:** `iv-panic-boundary` (ERR 3), `iv-oneshot-thread` (CONC 8). **Used again in:** `iv-parallel-mapreduce` (CONC 13), `iv-job-scheduler` (CONC 15), `iv-actor` (CONC 16), `iv-work-stealing` (CONC 19), `iv-graceful-shutdown` (ASYNC 7), `iv-tcp-echo-framed` (NET 5), `iv-http-server` (NET 7).
- **Cookbook:** exact `concurrency-interview-questions` (its Q12: no result handles). **Status:** exists: `c2-thread-pool`. **Runner:** gradable today.

#### CONC 10. Sharded concurrent map

`iv-sharded-map` · 60 min · medium · P1 · Tracks: C1

- **New idea:** splitting one lock into many.
- **Core:** insert, get, remove through &self with N locks.
- **Follow-up 1:** Atomic update and get_or_insert_with. *Test:* exact counters under contention.
- **Follow-up 2:** Consistent len or snapshot across shards. *Test:* lock order.
- **Follow-up 3:** Resize one shard at a time. *Test:* model.
- **Trade-offs:** Shard count; false sharing. **Concepts:** lock striping, hashing.
- **After this:** you can reduce contention. **In projects:** in-memory stores.
- **Required first:** `iv-hashmap-chaining` (COLL 3).
- **Cookbook:** similar\* `sharded-map`, similar `concurrent-hashmap-designs`. **Status:** to be written. **Runner:** gradable today.

#### CONC 11. Compute-once cache under concurrency

`iv-singleflight` · 60 min · medium · P1 · Tracks: C1

- **New idea:** concurrent callers share one computation.
- **Core:** get_or_load(key, f): concurrent callers share one load.
- **Follow-up 1:** Errors and panics release waiters. *Test:* no hang, retry works.
- **Follow-up 2:** TTL with fake clock. *Test:* load counts.
- **Follow-up 3:** Async version. *Test:* tokio paused time.
- **Trade-offs:** Holding locks during load. **Concepts:** Condvar slots, entry API, OnceLock.
- **After this:** you can stop cache stampedes. **In projects:** caches and gateways.
- **Required first:** `iv-blocking-queue` (CONC 2). **Helpful:** `iv-memoize` (ITER 3). **Used again in:** `iv-async-batcher` (ASYNC 9), `iv-idempotency-store` (RESIL 4).
- **Cookbook:** exact `concurrency-interview-questions` (its Q22: success path only). **Status:** to be written (partly practised by `c3-once-cell`). **Runner:** gradable today.

#### CONC 12. In-process pub/sub

`iv-pubsub` · 60 min · medium · P1 · Tracks: C2

- **New idea:** fan-out with a policy for slow readers.
- **Core:** subscribe to a topic, publish to all subscribers over channels.
- **Follow-up 1:** Unsubscribe on drop. *Test:* counts after drops.
- **Follow-up 2:** Slow subscriber policy: drop oldest, drop new or block. *Test:* exact delivered and dropped counts.
- **Follow-up 3:** Wildcard topics. *Test:* matcher oracle.
- **Trade-offs:** Fan-out cost; backpressure. **Concepts:** channels, Weak or ids, backpressure.
- **After this:** you can broadcast without unbounded memory. **In projects:** event buses.
- **Required first:** `iv-channel-mpsc` (CONC 7). **Helpful:** `iv-trie` (COLL 6). **Used again in:** `iv-service-registry` (RESIL 6).
- **Cookbook:** exact `systems-design-in-code` (its Q9: log with retention and cursors), related `crossbeam-select-channels`. **Status:** to be written. **Runner:** gradable today.

#### CONC 13. Parallel word count and merge sort

`iv-parallel-mapreduce` · 60 min · medium · P2 · Tracks: C6

- **New idea:** split, run in scoped threads, merge in a fixed order.
- **Core:** Count words with scoped threads and merge maps.
- **Follow-up 1:** Parallel merge sort with a sequential cutoff. *Test:* equals std sort, thread count bounded.
- **Follow-up 2:** Same with rayon. *Test:* identical results.
- **Follow-up 3:** Deterministic reduction order. *Test:* non-associative detector.
- **Trade-offs:** Granularity. **Concepts:** scoped threads, divide and conquer.
- **After this:** you can parallelise batch work. **In projects:** analytics.
- **Required first:** `iv-thread-pool` (CONC 9). **Used again in:** `iv-work-stealing` (CONC 19), `iv-chunk-queue` (CONC 20).
- **Cookbook:** similar\* `concurrency-interview-questions` (its Q13 and Q14), related `rayon-data-parallelism`. **Status:** to be written. **Runner:** ok (rayon is vendored).

#### CONC 14. Backpressured processing pipeline

`iv-pipeline-stages` · 60 min · medium · P2 · Tracks: C2

- **New idea:** bounded stages push back and cancel together.
- **Core:** Three stages over bounded channels with fan-out in the middle.
- **Follow-up 1:** Order preserved despite parallel stage. *Test:* sequence numbers.
- **Follow-up 2:** Error cancels the whole pipeline. *Test:* all threads join.
- **Follow-up 3:** Metrics per stage. *Test:* counts conserve.
- **Trade-offs:** Where to buffer. **Concepts:** channels, shutdown, reordering buffer.
- **After this:** you can build streaming pipelines. **In projects:** ingest services.
- **Required first:** `iv-channel-mpsc` (CONC 7).
- <details><summary>Links hidden until asked</summary>helpful: `iv-binary-heap` (COLL 4)</details>
- **Cookbook:** similar `crossbeam-select-channels` (four-stage pipeline). **Status:** to be written. **Runner:** gradable today.

#### CONC 15. Delayed and recurring job scheduler

`iv-job-scheduler` · 60 min · medium · P1 · Tracks: S5

- **New idea:** a time-ordered queue with cancellation and repeats.
- **Core:** schedule(job, at) and run_due(now) with a fake clock, ties FIFO.
- **Follow-up 1:** Priorities among due jobs. *Test:* oracle.
- **Follow-up 2:** Cancel and reschedule. *Test:* random ops against a sorted-Vec model.
- **Follow-up 3:** Recurring jobs without drift or pile-up. *Test:* long fake-time runs.
- **Trade-offs:** Heap with lazy cancel vs indexed heap vs wheel. **Concepts:** priority queue, fake clock, cancellation.
- **After this:** you can schedule delayed work. **In projects:** cron and retry queues.
- **Helpful:** `iv-indexed-pq` (COLL 8), `iv-thread-pool` (CONC 9), `iv-hit-counter` (CACHE 3). **Used again in:** `iv-timing-wheel` (CONC 18), `iv-executor` (ASYNC 2), `iv-cpu-scheduler` (OS 1), `iv-reliable-udp` (NET 3), `iv-visibility-queue` (DIST 3).
- <details><summary>Links hidden until asked</summary>required first: `iv-binary-heap` (COLL 4); used again in: `iv-sim-network` (DIST 1), `iv-stream-windows` (DIST 13)</details>
- **Cookbook:** exact `systems-design-in-code` (its Q10: delayed plus priority), similar `timing-wheel`. **Status:** to be written. **Runner:** gradable today.

#### CONC 16. A tiny actor runtime on threads

`iv-actor` · 90 min · medium · P2 · Tracks: C2

- **New idea:** state owned by one thread, reached by messages.
- **Core:** spawn actor with a mailbox, send and ask.
- **Follow-up 1:** Supervision: restart on panic with a limit. *Test:* restart counts.
- **Follow-up 2:** Graceful stop drains mailboxes. *Test:* nothing lost.
- **Follow-up 3:** Bounded mailbox. *Test:* sender sees Full.
- **Trade-offs:** Actors vs shared state. **Concepts:** channels, ownership transfer, supervision.
- **After this:** you can design supervised services. **In projects:** connection managers.
- **Required first:** `iv-thread-pool` (CONC 9). **Helpful:** `iv-vending-machine` (PAT 1). **Used again in:** `iv-parked-waiters` (CONC 21), `iv-async-sync-bridge` (ASYNC 14), `iv-group-commit` (STORE 18).
- **Cookbook:** similar `actor-model` (the tokio version). **Status:** to be written. **Runner:** gradable today.

#### CONC 17. Reader-writer lock

`iv-rwlock` · 90 min · hard · P2 · Tracks: C1

- **New idea:** writer preference stops starvation.
- **Core:** Many readers or one writer with guards.
- **Follow-up 1:** Writer preference. *Test:* new readers wait once a writer waits.
- **Follow-up 2:** Upgradable read. *Test:* no deadlock between two upgraders by design.
- **Follow-up 3:** Poisoning. *Test:* panic while writing.
- **Trade-offs:** Starvation policies. **Concepts:** Condvar, guards, UnsafeCell.
- **After this:** you can choose lock fairness. **In projects:** read-heavy state.
- **Required first:** `iv-semaphore` (CONC 5). **Helpful:** `iv-my-refcell` (SMART 2). **Used again in:** `iv-latch-crabbing` (STORE 15).
- **Cookbook:** similar\* `concurrency-interview-questions` (its Q4: writer preference without guards). **Status:** exists: `c1-rwlock-writer-preferring`. **Runner:** gradable today.

#### CONC 18. Hierarchical timing wheel

`iv-timing-wheel` · 90 min · hard · P2 · Tracks: F4

- **New idea:** time as a ring of buckets with cascading.
- **Core:** insert, cancel and advance with one wheel and an overflow list.
- **Follow-up 1:** Cascading levels. *Test:* differential against a heap.
- **Follow-up 2:** O(1) cancel by handle. *Test:* stale handles ignored.
- **Follow-up 3:** Never early under coarse ticks. *Test:* property.
- **Trade-offs:** Wheel vs heap. **Concepts:** ring of buckets, generational handles.
- **After this:** you can hold millions of timers. **In projects:** network servers.
- **Required first:** `iv-job-scheduler` (CONC 15). **Helpful:** `iv-slab` (MEM 1), `iv-ring-deque` (COLL 2).
- **Cookbook:** similar\* `timing-wheel`. **Status:** to be written. **Runner:** gradable today.

#### CONC 19. Work-stealing scheduler

`iv-work-stealing` · 120 min · hard · P2 · Tracks: C2

- **New idea:** per-worker queues with stealing.
- **Core:** Per-worker deques with stealing, fork-join of a recursive sum.
- **Follow-up 1:** Scoped spawn that borrows the stack. *Test:* borrowing tests.
- **Follow-up 2:** Panic propagation at join. *Test:* workers survive.
- **Follow-up 3:** Idle parking without lost wakeups. *Test:* stress with many tiny batches.
- **Trade-offs:** LIFO local, FIFO steal. **Concepts:** work stealing, parking.
- **After this:** you can explain async and rayon schedulers. **In projects:** runtimes.
- **Required first:** `iv-thread-pool` (CONC 9). **Helpful:** `iv-parallel-mapreduce` (CONC 13).
- **Cookbook:** similar\* `thread-pool-work-stealing`. **Status:** to be written. **Runner:** ok (crossbeam is vendored).

#### CONC 20. Hand out disjoint mutable chunks to threads

`iv-chunk-queue` · 45 min · medium · P2 · prep · Tracks: CONC

- **New idea:** a locked iterator over chunks_mut balances load without sharing mutable data.
- **Core:** Fill a buffer with uneven per-element cost using scoped threads that pull chunks from a Mutex-guarded iterator.
- **Follow-up 1:** keep the lock only while taking a chunk. *Test:* workers rendezvous inside work to prove two run at once ; panic in one worker :: the scope reports it and nothing deadlocks.
- **Required first:** `iv-parallel-mapreduce` (CONC 13).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

#### CONC 21. Park requests in a single-threaded server and resume them

`iv-parked-waiters` · 90 min · hard · P2 · prep · Tracks: CONC

- **New idea:** a blocked request is stored state that a later event resumes.
- **Core:** In a single-threaded state machine, implement blocking pop over several queues: store the remaining batch, keep a FIFO per key, and on push serve the oldest waiter and resume its batch through a ready list instead of recursion.
- **Follow-up 1:** timeout and cancellation. *Test:* injected time, waiter removed from every key queue and no element lost ; pipelined order and chained wake-ups :: seeded scripts compared with a naive model for reply order.
- **Required first:** `iv-actor` (CONC 16).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

#### CONC 22. Put idle workers to sleep without missing work

`iv-park-wake-handshake` · 90 min · hard · P2 · prep · Tracks: CONC

- **New idea:** a sleeper announces itself and rechecks for work before parking, while a producer publishes work before looking for sleepers.
- **Core:** Build the idle and wake protocol around several lock-free queues with a sleeper count, fence and condvar, with no timed fallback.
- **Follow-up 1:** shutdown flag through the same protocol. *Test:* all threads join ; step-controlled hooks between publish, announce and recheck :: every forced interleaving processes the item.
- **Required first:** `iv-blocking-queue` (CONC 2), `iv-spinlock` (ATOM 1).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

### ATOM: Atomics and lock-free structures

Memory ordering by building things that need it. *Core series, 9 parts, about 12 h. Ends in `cap-unsafekit`.* Alongside it: `iv-interleaving-checker` (ATOM 9).

#### ATOM 1. Spinlock

`iv-spinlock` · 45 min · medium · P1 · Tracks: C3

- **New idea:** acquire and release make a critical section.
- **Core:** lock with a guard on AtomicBool.
- **Follow-up 1:** try_lock and into_inner. *Test:* unit tests.
- **Follow-up 2:** Backoff. *Test:* still correct under contention.
- **Follow-up 3:** Ticket lock fairness. *Test:* order of tickets.
- **Trade-offs:** Acquire and Release pairing. **Concepts:** atomics, ordering, guards.
- **After this:** you can read ordering code. **In projects:** kernels and runtimes.
- **Required first:** `iv-blocking-queue` (CONC 2). **Helpful:** `iv-raii-pool` (SMART 1). **Used again in:** `iv-park-wake-handshake` (CONC 22), `iv-once` (ATOM 2), `iv-arc` (ATOM 4), `iv-spsc` (ATOM 5), `iv-seqlock` (ATOM 6), `iv-treiber` (ATOM 8), `iv-interleaving-checker` (ATOM 9), `iv-atomic-counters` (MON 1).
- **Cookbook:** similar\* `implement-std-from-scratch` (its Q10), similar `mutex-condvar-channel`. **Status:** exists: `c3-spinlock`. **Runner:** ok; weak-memory bugs need loom or Miri.

#### ATOM 2. Once cell and lazy value

`iv-once` · 60 min · hard · P2 · Tracks: C3

- **New idea:** a state machine in one atomic.
- **Core:** get_or_init exactly once across threads.
- **Follow-up 1:** Panic leaves it reusable. *Test:* scripted.
- **Follow-up 2:** Waiters park instead of spin. *Test:* stress.
- **Follow-up 3:** Lazy<T> on top. *Test:* init counts.
- **Trade-offs:** State machine in an atomic. **Concepts:** atomics, once initialisation.
- **After this:** you can initialise exactly once. **In projects:** lazy globals.
- **Required first:** `iv-spinlock` (ATOM 1). **Used again in:** `iv-task-wake-state` (ASYNC 15).
- **Cookbook:** similar `concurrency-interview-questions` (its Q10). **Status:** exists: `c3-once-cell`. **Runner:** ok; loom needed to prove ordering.

#### ATOM 3. Read-mostly config with snapshot swap

`iv-rcu-snapshot` · 45 min · medium · P2 · Tracks: C3

- **New idea:** readers keep old snapshots while writers swap.
- **Core:** readers get an Arc snapshot; writer swaps.
- **Follow-up 1:** update(closure) with no lost updates. *Test:* concurrent increments.
- **Follow-up 2:** Subscribe to changes. *Test:* every reader sees versions in order.
- **Follow-up 3:** Compare RwLock, mutex plus Arc and arc-swap. *Test:* same suite.
- **Trade-offs:** Grace periods. **Concepts:** RCU, Arc, copy-on-write.
- **After this:** you can serve read-mostly config. **In projects:** proxies and flags.
- **Required first:** `iv-raii-pool` (SMART 1). **Helpful:** `iv-my-rc` (SMART 3). **Used again in:** `iv-generation-cache` (CACHE 9).
- **Cookbook:** similar\* `snapshot-cow-rcu`, similar `read-copy-update-and-hazard-patterns`. **Status:** to be written. **Runner:** ok (arc-swap is vendored).

#### ATOM 4. Arc from scratch

`iv-arc` · 90 min · hard · P1 · Tracks: C3

- **New idea:** reference counts need acquire and release around drop.
- **Core:** clone, deref, drop with atomic counts.
- **Follow-up 1:** Weak with upgrade race safety. *Test:* stress while owners vanish.
- **Follow-up 2:** get_mut and try_unwrap. *Test:* uniqueness checks.
- **Follow-up 3:** make_mut. *Test:* clone-on-write.
- **Trade-offs:** Relaxed increment, Release decrement, Acquire fence. **Concepts:** atomics, refcounting, fences.
- **After this:** you can reason about shared lifetime across threads. **In projects:** any Arc code.
- **Required first:** `iv-my-rc` (SMART 3). **Helpful:** `iv-spinlock` (ATOM 1).
- **Cookbook:** similar\* `own-arc-refcount`. **Status:** to be written (partly practised by `c3-mini-arc`). **Runner:** ok; Miri or loom needed to check ordering.

#### ATOM 5. SPSC ring channel

`iv-spsc` · 90 min · hard · P1 · Tracks: C3

- **New idea:** one writer per index makes a ring wait-free.
- **Core:** push and pop wait-free over a fixed ring.
- **Follow-up 1:** Drop of remaining items. *Test:* drop counters.
- **Follow-up 2:** Cached head and tail to cut traffic. *Test:* same tests.
- **Follow-up 3:** Batch push and pop. *Test:* sequence check over 1e6 items.
- **Trade-offs:** Why Release and Acquire here; cache padding. **Concepts:** ring buffer, atomics, UnsafeCell, MaybeUninit.
- **After this:** you can move data between two threads fast. **In projects:** audio and telemetry.
- **Required first:** `iv-ring-deque` (COLL 2). **Helpful:** `iv-fixed-vec` (UNSAFE 4), `iv-spinlock` (ATOM 1). **Used again in:** `iv-mpmc` (ATOM 7), `iv-io-uring-model` (OS 6).
- **Cookbook:** similar\* `spsc-ring-buffer`. **Status:** exists: `c3-spsc-ring`. **Runner:** ok; loom or Miri needed for ordering bugs.

#### ATOM 6. Seqlock

`iv-seqlock` · 60 min · hard · P3 · Tracks: C3

- **New idea:** readers retry when a version changes.
- **Core:** One writer, many readers of a multi-word value with retry.
- **Follow-up 1:** Multiple writers via a lock. *Test:* consistency check.
- **Follow-up 2:** Reader progress under constant writes. *Test:* bounded retries not guaranteed, discuss.
- **Follow-up 3:** Fences placed explicitly. *Test:* review.
- **Trade-offs:** Why the data must be atomics in Rust. **Concepts:** sequence lock, fences.
- **After this:** you can publish small hot values. **In projects:** clocks and quotes.
- **Required first:** `iv-spinlock` (ATOM 1).
- **Cookbook:** similar\* `seqlock-and-priority-inversion`. **Status:** to be written. **Runner:** ok; torn reads need Miri or loom to be proven absent.

#### ATOM 7. Bounded lock-free MPMC queue

`iv-mpmc` · 120 min · hard · P2 · Tracks: C3

- **New idea:** per-cell sequence numbers order many writers.
- **Core:** Sequence-numbered cells, try_push and try_pop.
- **Follow-up 1:** Drop safety. *Test:* counters.
- **Follow-up 2:** Blocking wrapper. *Test:* no lost wakeup stress.
- **Follow-up 3:** Per-producer order. *Test:* tagged items.
- **Trade-offs:** Vyukov design; ABA. **Concepts:** sequence numbers, CAS loops.
- **After this:** you can build bounded lock-free queues. **In projects:** runtimes.
- **Required first:** `iv-spsc` (ATOM 5).
- **Cookbook:** similar\* `mpmc-bounded-queue`. **Status:** exists: `c3-bounded-mpmc`. **Runner:** ok; loom or Miri needed.

#### ATOM 8. ABA-safe lock-free stack

`iv-treiber` · 90 min · hard · P2 · Tracks: C3

- **New idea:** a reused slot fools compare-and-swap.
- **Core:** Index stack with a free list and tagged heads.
- **Follow-up 1:** Scripted ABA schedule on the untagged version. *Test:* failure reproduced deterministically.
- **Follow-up 2:** Stress conservation. *Test:* no value lost.
- **Follow-up 3:** Discuss reclamation (hazard pointers, epochs). *Test:* discussion.
- **Trade-offs:** Why pointers make it harder. **Concepts:** CAS, tagging, ABA.
- **After this:** you can spot ABA. **In projects:** lock-free free lists.
- **Required first:** `iv-spinlock` (ATOM 1). **Helpful:** `iv-slab` (MEM 1).
- **Cookbook:** similar\* `lockfree-stack-aba`. **Status:** exists: `c3-index-stack`. **Runner:** ok; loom or Miri needed.

#### ATOM 9. A mini model checker for interleavings

`iv-interleaving-checker` · 120 min · hard · P3 · Tracks: C3

- **New idea:** try every schedule of a small program.
- **Core:** Explore all schedules of small thread programs and report a failing one.
- **Follow-up 1:** State deduplication. *Test:* same verdicts, fewer states.
- **Follow-up 2:** Shortest counterexample. *Test:* BFS check.
- **Follow-up 3:** Store buffers (TSO). *Test:* store-buffering litmus appears.
- **Trade-offs:** What loom does. **Concepts:** state-space search, memory models.
- **After this:** you can test concurrency by search. **In projects:** lock-free libraries.
- **Required first:** `iv-spinlock` (ATOM 1). **Helpful:** `iv-alg-config-matrix` (REC 3).
- **Cookbook:** similar\* `model-checker-for-concurrent-code`. **Status:** to be written. **Runner:** gradable today.

### ASYNC: Async: from a Future to a small runtime

Build the parts, then use them correctly. *Core series, 17 parts, about 21 h. Ends in `cap-runtime`.* Alongside it: `iv-mon-spans` (MON 4), `iv-fake-clock` (TEST 1).

#### ASYNC 1. block_on with a hand-made waker

`iv-block-on` · 60 min · medium · P1 · Tracks: C5

- **New idea:** a waker tells the executor to poll again.
- **Core:** Drive one future on the current thread, sleeping when pending.
- **Follow-up 1:** Wake from another thread before park. *Test:* no lost wakeup.
- **Follow-up 2:** join two futures. *Test:* interleaving.
- **Follow-up 3:** select returning the first and dropping the other. *Test:* drop observed.
- **Trade-offs:** Wake, thread park, spurious polls. **Concepts:** Future, Waker, parking.
- **After this:** you can explain what await does. **In projects:** any async debugging.
- **Required first:** nothing. **Helpful:** `iv-blocking-queue` (CONC 2). **Used again in:** `iv-pin-mapfuture` (UNSAFE 7), `iv-executor` (ASYNC 2), `iv-send-across-await` (ASYNC 4), `iv-future-combinators` (ASYNC 16).
- **Cookbook:** similar\* `futures-by-hand`, similar `waker-executor`. **Status:** exists: `c5-block-on`. **Runner:** gradable today.

#### ASYNC 2. Single-threaded executor

`iv-executor` · 90 min · hard · P1 · Tracks: C5

- **New idea:** tasks queued by their own wakers.
- **Core:** spawn and run tasks with a ready queue and wakers.
- **Follow-up 1:** JoinHandle as a future. *Test:* results and panics.
- **Follow-up 2:** Timers on a manual clock. *Test:* wake order.
- **Follow-up 3:** Never enqueue a task twice. *Test:* poll counts.
- **Trade-offs:** Task state machine; Arc wakers. **Concepts:** executor, wakers, task queue.
- **After this:** you can read runtime code. **In projects:** tokio-like internals.
- **Required first:** `iv-block-on` (ASYNC 1). **Helpful:** `iv-job-scheduler` (CONC 15), `iv-oneshot-thread` (CONC 8). **Used again in:** `iv-async-channel` (ASYNC 3), `iv-task-wake-state` (ASYNC 15), `iv-io-uring-model` (OS 6), `iv-event-loop` (NET 8).
- **Cookbook:** exact `waker-executor` (executor with a thread-backed timer), similar `mini-async-runtime`. **Status:** exists: `c5-mini-executor`. **Runner:** gradable today.

#### ASYNC 3. Async bounded channel

`iv-async-channel` · 90 min · hard · P1 · Tracks: C5

- **New idea:** store a waker where the event will happen.
- **Core:** send().await and recv().await with wakers.
- **Follow-up 1:** Disconnect. *Test:* errors.
- **Follow-up 2:** Cancel safety of a dropped send or recv future. *Test:* no lost message or waker leak.
- **Follow-up 3:** Many senders fairness. *Test:* order of waiting.
- **Trade-offs:** Where wakers live. **Concepts:** wakers, backpressure, cancel safety.
- **After this:** you can write async primitives. **In projects:** libraries.
- **Required first:** `iv-executor` (ASYNC 2). **Helpful:** `iv-channel-mpsc` (CONC 7). **Used again in:** `iv-async-batcher` (ASYNC 9), `iv-async-primitives` (ASYNC 10), `iv-register-recheck` (ASYNC 17).
- **Cookbook:** similar `mini-async-runtime` (oneshot, mpsc, semaphore). **Status:** to be written (partly practised by `c5-oneshot-channel`). **Runner:** gradable today.

#### ASYNC 4. Fix a server that does not compile for spawn

`iv-send-across-await` · 45 min · medium · P2 · Tracks: C4

- **New idea:** what a future holds across await decides Send.
- **Core:** Remove non-Send state held across await.
- **Follow-up 1:** Replace a std mutex held across await. *Test:* deadlock reproduced on a current-thread runtime.
- **Follow-up 2:** Move CPU work to spawn_blocking. *Test:* timer ticks keep firing.
- **Follow-up 3:** Bound concurrency. *Test:* max in flight.
- **Trade-offs:** Which mutex. **Concepts:** Send, await points, blocking.
- **After this:** you can fix spawn errors. **In projects:** async services.
- **Required first:** `iv-block-on` (ASYNC 1). **Used again in:** `iv-fanout-limit` (ASYNC 5), `iv-tokio-job-outcome` (ASYNC 13).
- **Cookbook:** related `rust-async-qa`, related `async-patterns`. **Status:** to be written. **Runner:** ok; best as a fix-mode problem.

#### ASYNC 5. Fetch many with a concurrency limit

`iv-fanout-limit` · 60 min · medium · P1 · Tracks: C4

- **New idea:** many futures with a cap on in flight.
- **Core:** Run N async jobs with at most K in flight.
- **Follow-up 1:** Preserve input order. *Test:* oracle.
- **Follow-up 2:** Fail fast and cancel the rest. *Test:* started and dropped counts.
- **Follow-up 3:** Per-job timeout and retry. *Test:* paused time.
- **Trade-offs:** spawn vs buffer_unordered. **Concepts:** JoinSet, Semaphore, cancellation.
- **After this:** you can fetch in parallel safely. **In projects:** crawlers and gateways.
- **Required first:** `iv-send-across-await` (ASYNC 4). **Helpful:** `iv-semaphore` (CONC 5). **Used again in:** `iv-cancel-safety` (ASYNC 6), `iv-graceful-shutdown` (ASYNC 7).
- **Cookbook:** similar\* `joinset-semaphore` (limit with JoinSet), related `async-streams-concurrency`. **Status:** staged draft: `c4-bounded-fan-out`. **Runner:** ok (tokio vendored, paused time).

#### ASYNC 6. Cancel-safe frame reader

`iv-cancel-safety` · 60 min · medium · P1 · Tracks: C4

- **New idea:** dropping a future throws away its state.
- **Core:** Read length-prefixed frames inside select! without losing bytes.
- **Follow-up 1:** Shutdown signal. *Test:* partial frame preserved.
- **Follow-up 2:** Max frame size. *Test:* error.
- **Follow-up 3:** Batcher that flushes on size or time. *Test:* dropped future loses nothing (paused time).
- **Trade-offs:** What a losing branch drops. **Concepts:** cancel safety, buffering.
- **After this:** you can write correct select loops. **In projects:** protocol handlers.
- **Required first:** `iv-length-codec` (BIN 2). **Helpful:** `iv-fanout-limit` (ASYNC 5). **Used again in:** `iv-tokio-tcp-conn` (NET 9).
- **Cookbook:** similar `async-cancel-safety`, related `bytes-buf-buf-mut-framing`. **Status:** staged draft: `c4-fix-cancel-safe-reader`. **Runner:** gradable today.

#### ASYNC 7. Graceful shutdown of a worker service

`iv-graceful-shutdown` · 60 min · medium · P1 · Tracks: C4

- **New idea:** cancel, drain, then abort at a deadline.
- **Core:** Stop intake, drain queued work, exit.
- **Follow-up 1:** Deadline with abort of stragglers. *Test:* report of finished and aborted.
- **Follow-up 2:** Child tasks cancelled by a token tree. *Test:* no task left.
- **Follow-up 3:** Ordered shutdown of dependencies. *Test:* event order.
- **Trade-offs:** Cancellation vs abort. **Concepts:** CancellationToken, JoinSet, ordering.
- **After this:** you can stop services without loss. **In projects:** deploys.
- **Required first:** `iv-fanout-limit` (ASYNC 5). **Helpful:** `iv-thread-pool` (CONC 9). **Used again in:** `iv-async-retry-hedge` (ASYNC 12), `iv-http-server` (NET 7).
- **Cookbook:** similar `structured-concurrency-cancellation`, related `graceful-shutdown`. **Status:** staged draft: `c4-drain-with-deadline`. **Runner:** ok (tokio, tokio-util).

#### ASYNC 8. Write a Stream and adapters

`iv-stream-adapters` · 60 min · medium · P2 · Tracks: C4

- **New idea:** asynchronous iteration with backpressure.
- **Core:** A paginated source as a Stream.
- **Follow-up 1:** throttle and timeout adapters. *Test:* paused time.
- **Follow-up 2:** buffer_unordered by hand. *Test:* max in flight.
- **Follow-up 3:** Backpressure to the source. *Test:* pages requested counter.
- **Trade-offs:** Stream vs channel. **Concepts:** Stream, pinning.
- **After this:** you can process unbounded sources. **In projects:** pagination and ingest.
- **Required first:** `iv-iter-adapters` (ITER 1). **Helpful:** `iv-pin-mapfuture` (UNSAFE 7).
- **Cookbook:** similar `async-streams-concurrency`. **Status:** to be written. **Runner:** ok (futures vendored).

#### ASYNC 9. DataLoader-style request batcher

`iv-async-batcher` · 90 min · hard · P2 · Tracks: C4

- **New idea:** collect callers for one tick and answer together.
- **Core:** load(key).await batches keys requested in the same tick.
- **Follow-up 1:** Max batch size and delay. *Test:* paused time.
- **Follow-up 2:** Dedup keys and per-request cache. *Test:* backend call counts.
- **Follow-up 3:** Errors fan out to every waiter. *Test:* scripted.
- **Trade-offs:** Latency vs batch size. **Concepts:** oneshot, spawn, batching.
- **After this:** you can remove N+1 calls. **In projects:** GraphQL and databases.
- **Required first:** `iv-async-channel` (ASYNC 3). **Helpful:** `iv-singleflight` (CONC 11).
- **Cookbook:** similar\* `jsonrpc-graphql-dataloader` (its DataLoader). **Status:** to be written. **Runner:** ok (tokio).

#### ASYNC 10. Async semaphore, notify and timeout

`iv-async-primitives` · 120 min · hard · P2 · Tracks: C5

- **New idea:** cancelled waiters must give their turn back.
- **Core:** Semaphore with an acquire future.
- **Follow-up 1:** Dropped waiter does not lose a permit. *Test:* cancel at each step by manual polling.
- **Follow-up 2:** Notify with no lost notification. *Test:* scripted polls.
- **Follow-up 3:** timeout combinator. *Test:* paused time.
- **Trade-offs:** FIFO waiter lists. **Concepts:** intrusive waiter queues, cancellation.
- **After this:** you can write cancel-safe synchronisation. **In projects:** async libraries.
- **Required first:** `iv-async-channel` (ASYNC 3). **Helpful:** `iv-semaphore` (CONC 5).
- **Cookbook:** similar `mini-async-runtime`. **Status:** to be written. **Runner:** gradable today.

#### ASYNC 11. Object-safe async traits

`iv-async-trait-object` · 45 min · medium · P3 · Tracks: C4

- **New idea:** async methods behind dyn need boxed futures.
- **Core:** A Store trait with async fn, used generically.
- **Follow-up 1:** dyn-compatible adapter with boxed futures. *Test:* heterogeneous list.
- **Follow-up 2:** Send bounds for spawn. *Test:* tasks across threads.
- **Follow-up 3:** Wrapper that times out any Store. *Test:* paused time.
- **Trade-offs:** async fn in traits limits. **Concepts:** async traits, Pin<Box>, Send.
- **After this:** you can design async plugin points. **In projects:** storage backends.
- **Required first:** `iv-shape-registry` (TRAIT 1). **Used again in:** `iv-middleware-tower` (WEB 4).
- **Cookbook:** similar\* `async-traits-dyn` (adapter). **Status:** to be written. **Runner:** gradable today.

#### ASYNC 12. Retry, hedging and circuit breaking in async

`iv-async-retry-hedge` · 90 min · medium · P2 · Tracks: C4

- **New idea:** retries and hedges share one deadline budget.
- **Core:** retry with exponential backoff and jitter on retryable errors.
- **Follow-up 1:** Deadline budget across attempts. *Test:* paused time.
- **Follow-up 2:** Hedged second request that cancels the loser. *Test:* drop observed.
- **Follow-up 3:** Retry budget under outage. *Test:* call volume bound.
- **Trade-offs:** Idempotency. **Concepts:** retries, select, budgets.
- **After this:** you can call flaky dependencies. **In projects:** service clients.
- **Required first:** `iv-retry-backoff` (RESIL 1). **Helpful:** `iv-circuit-breaker` (RESIL 3), `iv-graceful-shutdown` (ASYNC 7).
- **Cookbook:** similar `api-gateway-service-discovery`, similar `http-client-resilience`. **Status:** to be written. **Runner:** ok (tokio).

#### ASYNC 13. Run one cancellable, panic-safe async job

`iv-tokio-job-outcome` · 90 min · hard · P2 · prep · Tracks: ASYNC

- **New idea:** a supervisor races a child task against a cancel signal and records exactly one outcome.
- **Core:** Spawn work in a child task and report succeeded, failed, panicked or cancelled using select!, a watch channel and AbortHandle, under paused tokio time.
- **Follow-up 1:** cancel while waiting for a semaphore permit. *Test:* work never starts and permits all return ; await the aborted child and define cancel-versus-finish priority with biased select :: a drop probe shows the child is gone before the outcome is recorded.
- **Required first:** `iv-send-across-await` (ASYNC 4), `iv-panic-boundary` (ERR 3).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

#### ASYNC 14. Connect async tasks to one blocking worker thread

`iv-async-sync-bridge` · 45 min · medium · P2 · prep · Tracks: ASYNC

- **New idea:** channels carry work between a runtime and a plain thread without blocking either.
- **Core:** Run a std worker thread that owns a counter map, fed by a sync channel from Tokio tasks, answering through oneshot channels, with an acknowledged shutdown and a spawn_blocking join.
- **Follow-up 1:** push events to async consumers with try_send. *Test:* full queue is reported and the worker never blocks ; dropped receivers and worker exit :: callers get an error and do not hang.
- **Required first:** `iv-actor` (CONC 16).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

#### ASYNC 15. Schedule each task at most once and lose no wake

`iv-task-wake-state` · 90 min · hard · P2 · prep · Tracks: ASYNC

- **New idea:** one atomic state decides who may enqueue a task and records a wake that arrives during a poll.
- **Core:** Implement notify and run over IDLE, SCHEDULED, RUNNING, NOTIFIED and COMPLETE against a recording scheduler that the test steps by hand.
- **Follow-up 1:** abort that drops the future on its next run. *Test:* drop guard counter and no further polls ; wake from many joined threads :: queue never holds the task twice and the exclusivity flag never trips.
- **Required first:** `iv-executor` (ASYNC 2), `iv-once` (ATOM 2).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

#### ASYNC 16. Write join and select by hand

`iv-future-combinators` · 90 min · hard · P2 · prep · Tracks: ASYNC

- **New idea:** one future drives several children under one waker and decides which to keep, store or drop.
- **Core:** Implement join of two, join of many and a biased select2 with boxed children, tested with scripted futures.
- **Follow-up 1:** timeout built from select. *Test:* scripted timer future ; fairness variant that alternates which side is polled first :: poll log.
- **Required first:** `iv-block-on` (ASYNC 1).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

#### ASYNC 17. Connect a lock-free event source to a waker

`iv-register-recheck` · 90 min · hard · P2 · prep · Tracks: ASYNC

- **New idea:** publish the waker and then recheck a flag or epoch so an event between check and sleep is not lost.
- **Core:** Implement poll_io over a fake edge-triggered source with epoch and waker slot: try, register on WouldBlock, recheck, retry.
- **Follow-up 1:** one-shot fired flag future. *Test:* hook fires the event between check and register ; separate read and write directions :: waking one leaves the other waiting.
- **Required first:** `iv-async-channel` (ASYNC 3).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

### PARSE: Parsers: from tokens to full formats

One new parsing idea at a time, ending with recovery. *Core series, 22 parts, about 26 h. Ends in `cap-sql`.* Alongside it: `iv-fuzz-harness` (TEST 5), `iv-shrinker` (TEST 3).

#### PARSE 1. RPN and infix calculator with a stack

`iv-rpn-calc` · 45 min · easy · P1 · Tracks: S5

- **New idea:** a stack evaluates nested expressions.
- **Core:** Evaluate reverse Polish notation with typed errors.
- **Follow-up 1:** Infix with + - and parentheses, unary minus. *Test:* random expression trees as oracle.
- **Follow-up 2:** Precedence for * and /. *Test:* oracle.
- **Follow-up 3:** Checked arithmetic. *Test:* overflow and divide by zero.
- **Trade-offs:** Two stacks vs shunting-yard. **Concepts:** stacks, error handling.
- **After this:** you can evaluate rules. **In projects:** calculators and filters.
- **Required first:** nothing. **Used again in:** `iv-recursive-descent` (PARSE 11).
- **Cookbook:** similar\* `stacks-queues-heaps-questions` (its Q4 and Q5). **Status:** to be written. **Runner:** gradable today.

#### PARSE 2. Tokenizer that borrows its input

`iv-tokenizer-borrowed` · 45 min · easy · P1 · Tracks: L3

- **New idea:** split text into spanned tokens that borrow it.
- **Core:** An Iterator of tokens with &str slices: identifiers, numbers, punctuation.
- **Follow-up 1:** Spans with line and column. *Test:* slicing the source by span gives the token.
- **Follow-up 2:** Errors instead of skipping unknown bytes, then continue. *Test:* all errors reported.
- **Follow-up 3:** Strings with escapes returning Cow. *Test:* no allocation when there are no escapes.
- **Trade-offs:** char_indices vs bytes. **Concepts:** zero-copy, lifetimes, iterator.
- **After this:** you can write any lexer. **In projects:** config and query languages.
- **Required first:** nothing. **Helpful:** `iv-view-splitter` (OWN 3). **Used again in:** `iv-ini-toml` (PARSE 4), `iv-url-parser` (PARSE 6), `iv-recursive-descent` (PARSE 11), `iv-markdown` (PARSE 14), `iv-sexpr-lisp` (LANG 1).
- <details><summary>Links hidden until asked</summary>used again in: `iv-template-engine` (TEXT 5)</details>
- **Cookbook:** exact `zero-copy-lexer` (borrowing token iterator; it skips unknown bytes). **Status:** exists: `l3-zero-copy-tokenizer`. **Runner:** gradable today.

#### PARSE 3. CSV parser as a state machine

`iv-csv` · 60 min · medium · P1 · Tracks: S2

- **New idea:** a state machine reads quoted text.
- **Core:** Quotes, escaped quotes, newlines inside quotes.
- **Follow-up 1:** Errors with line numbers. *Test:* table.
- **Follow-up 2:** Writer and round trip. *Test:* random fields.
- **Follow-up 3:** Streaming over BufRead with bounded memory. *Test:* chunked reader mock.
- **Trade-offs:** States vs split. **Concepts:** state machine, round trip.
- **After this:** you can parse formats with escapes. **In projects:** data import.
- **Required first:** nothing. **Used again in:** `iv-shell-line` (PARSE 8).
- **Cookbook:** exact `csv-state-machine` (parse, write, round trip). **Status:** to be written. **Runner:** gradable today.

#### PARSE 4. INI and TOML-subset parser

`iv-ini-toml` · 60 min · medium · P2 · Tracks: S2

- **New idea:** keys build a nested tree with conflict errors.
- **Core:** Sections, key = value, comments.
- **Follow-up 1:** Typed values: strings, ints, bools, arrays. *Test:* tables.
- **Follow-up 2:** Dotted keys into nested tables with conflict errors. *Test:* oracle.
- **Follow-up 3:** Errors with line and column, collect all. *Test:* positions.
- **Trade-offs:** Line-based vs token-based. **Concepts:** parsing, maps, diagnostics.
- **After this:** you can read config files. **In projects:** tool configuration.
- **Required first:** `iv-tokenizer-borrowed` (PARSE 2). **Helpful:** `iv-diagnostics-renderer` (ERR 1).
- **Cookbook:** related `serde-attributes-enum-representations`, related `diagnostics-spans`. **Status:** to be written (partly practised by `s2-fromstr-setting`). **Runner:** gradable today.

#### PARSE 5. Version and constraint parser

`iv-semver` · 45 min · easy · P3 · Tracks: S8

- **New idea:** parsing into a type whose ordering has rules.
- **Core:** Parse semver and order it.
- **Follow-up 1:** Pre-release precedence. *Test:* spec examples.
- **Follow-up 2:** Constraints ^ ~ >= with AND. *Test:* brute force over small versions.
- **Follow-up 3:** FromStr, Display round trip. *Test:* property.
- **Trade-offs:** Manual Ord. **Concepts:** FromStr, Ord.
- **After this:** you can implement FromStr and Ord. **In projects:** package tools.
- **Required first:** nothing.
- **Cookbook:** related `std-cmp-hash-default-traits`, related `std-num-parsing-formatting`. **Status:** to be written. **Runner:** gradable today.

#### PARSE 6. URL parser

`iv-url-parser` · 60 min · medium · P2 · Tracks: S2 L3

- **New idea:** validate while borrowing, decode only when needed.
- **Core:** scheme, host, port, path, query borrowed from the input.
- **Follow-up 1:** Percent decoding with Cow. *Test:* allocation counting.
- **Follow-up 2:** IPv6 literals and userinfo. *Test:* tables.
- **Follow-up 3:** Relative reference resolution. *Test:* RFC examples.
- **Trade-offs:** What to reject. **Concepts:** zero-copy, validation.
- **After this:** you can parse untrusted addresses. **In projects:** proxies and clients.
- **Required first:** `iv-tokenizer-borrowed` (PARSE 2). **Helpful:** `iv-base64` (TEXT 3). **Used again in:** `iv-http-request` (NET 6), `iv-sec-safe-path` (SEC 1).
- **Cookbook:** similar `encodings-codecs` (percent coding). **Status:** to be written (partly practised by `s2-parse-url-borrowed`). **Runner:** gradable today.

#### PARSE 7. Command-line argument parser

`iv-cli-args` · 60 min · medium · P2 · Tracks: S10

- **New idea:** tokens come already split, meaning is state.
- **Core:** Flags, options with values, positionals.
- **Follow-up 1:** Combined short flags and --. *Test:* tables.
- **Follow-up 2:** Subcommands and help text. *Test:* golden.
- **Follow-up 3:** Typed getters with errors. *Test:* property over generated argv.
- **Trade-offs:** Declarative vs imperative. **Concepts:** state machine, builder.
- **After this:** you can build command-line tools. **In projects:** every CLI.
- **Required first:** nothing.
- **Cookbook:** similar `std-env-args-vars` (hand-written CLI parser), related `clap-cli`. **Status:** to be written. **Runner:** ok (clap vendored; here by hand).

#### PARSE 8. Shell command line parser

`iv-shell-line` · 60 min · medium · P2 · Tracks: S2

- **New idea:** quoting changes the lexer mode.
- **Core:** Words, quotes and escapes.
- **Follow-up 1:** Variable expansion rules per quote type. *Test:* tables.
- **Follow-up 2:** Pipes and redirections into a struct. *Test:* golden.
- **Follow-up 3:** Unterminated quote errors with position. *Test:* table.
- **Trade-offs:** Lexer modes. **Concepts:** state machine, enums.
- **After this:** you can parse command lines. **In projects:** shells and job runners.
- **Required first:** `iv-csv` (PARSE 3). **Used again in:** `iv-sbx-command-policy` (SBX 2).
- **Cookbook:** related `process-pipes-signals`. **Status:** exists: `y9-shell-line-parser`. **Runner:** gradable today.

#### PARSE 9. RFC 3339 and date arithmetic

`iv-datetime-parse` · 45 min · medium · P3 · Tracks: S10

- **New idea:** calendar arithmetic behind a fixed format.
- **Core:** Parse and format timestamps.
- **Follow-up 1:** days-from-civil and back. *Test:* round trip over a wide range.
- **Follow-up 2:** Offsets. *Test:* tables.
- **Follow-up 3:** Month arithmetic policy. *Test:* tables.
- **Trade-offs:** Wall vs monotonic time. **Concepts:** parsing, calendar maths.
- **After this:** you can handle timestamps. **In projects:** logs and APIs.
- **Required first:** nothing.
- **Cookbook:** similar\* `civil-dates-time`. **Status:** to be written. **Runner:** gradable today.

#### PARSE 10. Glob matcher

`iv-glob` · 60 min · medium · P1 · Tracks: S2

- **New idea:** wildcards with backtracking that cannot explode.
- **Core:** * and ? matching in O(n*m) without exponential cases.
- **Follow-up 1:** Character classes and escapes. *Test:* tables.
- **Follow-up 2:** ** across path segments. *Test:* oracle by naive recursion on small inputs.
- **Follow-up 3:** Compile many patterns and match one path fast. *Test:* oracle.
- **Trade-offs:** Greedy backtracking vs DP vs NFA. **Concepts:** two pointers, DP.
- **After this:** you can match paths and topics. **In projects:** ignore files and routers.
- **Required first:** nothing. **Used again in:** `iv-regex-nfa` (PARSE 15), `iv-dir-walker` (IO 6), `iv-router` (WEB 1).
- **Cookbook:** similar `dp-and-backtracking-questions` (wildcard matching DP). **Status:** to be written. **Runner:** gradable today.

#### PARSE 11. Recursive descent expression evaluator

`iv-recursive-descent` · 60 min · medium · P1 · Tracks: S2

- **New idea:** one function per precedence level.
- **Core:** Grammar with + - * / and parentheses, one function per level.
- **Follow-up 1:** Variables and let bindings. *Test:* environment tests.
- **Follow-up 2:** Error positions. *Test:* exact offsets.
- **Follow-up 3:** Depth limit on hostile input. *Test:* 1e5 parentheses.
- **Trade-offs:** Left recursion; AST vs direct eval. **Concepts:** grammar, recursion, spans.
- **After this:** you can parse grammars by hand. **In projects:** rule engines.
- **Required first:** `iv-tokenizer-borrowed` (PARSE 2). **Helpful:** `iv-rpn-calc` (PARSE 1). **Used again in:** `iv-pratt` (PARSE 12), `iv-json` (PARSE 13), `iv-regex-nfa` (PARSE 15), `iv-parser-combinators` (PARSE 16), `iv-stmt-parser-desugar` (PARSE 19), `iv-regex-syntax` (PARSE 20), `iv-cps-backtracker` (PARSE 22), `iv-interp-call-frames` (LANG 13), `iv-resp` (BIN 4).
- **Cookbook:** similar `expression-parsing-three-ways`, similar `edge-dsl-parser`. **Status:** to be written. **Runner:** gradable today.

#### PARSE 12. Pratt parser

`iv-pratt` · 60 min · medium · P1 · Tracks: S2

- **New idea:** binding powers replace precedence functions.
- **Core:** Binding powers for infix and prefix operators.
- **Follow-up 1:** Right associativity and non-associative comparisons. *Test:* tree printing oracle.
- **Follow-up 2:** Calls, indexing and postfix operators. *Test:* golden trees.
- **Follow-up 3:** Print with minimal parentheses and round trip. *Test:* random trees.
- **Trade-offs:** Pratt vs precedence climbing vs shunting-yard. **Concepts:** binding power, AST.
- **After this:** you can add operators easily. **In projects:** query and formula languages.
- **Required first:** `iv-recursive-descent` (PARSE 11). **Used again in:** `iv-spreadsheet` (PAT 15), `iv-error-recovery` (PARSE 18), `iv-stmt-parser-desugar` (PARSE 19), `iv-constant-fold` (LANG 2), `iv-typecheck-simple` (LANG 3), `iv-tree-interpreter` (LANG 4), `iv-expr-eval-3vl` (QUERY 1), `iv-sql-parser` (QUERY 7), `iv-boolean-query` (SEARCH 3).
- **Cookbook:** exact `pratt-parser` (core loop on numbers), similar `expression-parsing-three-ways`. **Status:** staged draft: `y14-pratt-parser`. **Runner:** gradable today.

#### PARSE 13. JSON parser

`iv-json` · 120 min · medium · P1 · Tracks: S2

- **New idea:** a recursive format with hostile input limits.
- **Core:** Parse into an enum with positions.
- **Follow-up 1:** String escapes including surrogate pairs. *Test:* table tests.
- **Follow-up 2:** Depth limit and strict numbers. *Test:* hostile inputs, never panic.
- **Follow-up 3:** Serializer with round-trip property. *Test:* random values.
- **Trade-offs:** f64 limits; duplicate keys. **Concepts:** recursive descent, escapes, round-trip testing.
- **After this:** you can write a safe format parser. **In projects:** any wire format.
- **Required first:** `iv-recursive-descent` (PARSE 11). **Helpful:** `iv-fuzz-harness` (TEST 5). **Used again in:** `iv-json-streaming` (PARSE 17).
- **Cookbook:** exact `json-parser` (positions, escapes, depth limit). **Status:** to be written. **Runner:** gradable today.

#### PARSE 14. Markdown subset to HTML

`iv-markdown` · 120 min · medium · P2 · Tracks: S2

- **New idea:** block structure first, inline structure second.
- **Core:** Headings, paragraphs, lists, code fences.
- **Follow-up 1:** Inline emphasis and code spans. *Test:* golden corpus.
- **Follow-up 2:** Nested lists and quotes. *Test:* golden.
- **Follow-up 3:** HTML escaping and no panics on random text. *Test:* property.
- **Trade-offs:** Two-phase parsing. **Concepts:** block and inline parsing.
- **After this:** you can parse layered documents. **In projects:** docs tools.
- **Required first:** `iv-tokenizer-borrowed` (PARSE 2).
- **Cookbook:** similar\* `markdown-block-parser` (two-phase CommonMark subset). **Status:** to be written. **Runner:** gradable today.

#### PARSE 15. Regex engine with a Thompson NFA

`iv-regex-nfa` · 120 min · hard · P1 · Tracks: S2

- **New idea:** compile a pattern to states and run them all at once.
- **Core:** Literals, ., *, |, grouping; full match.
- **Follow-up 1:** + ? and classes. *Test:* differential against a backtracking oracle.
- **Follow-up 2:** Leftmost-longest find and anchors. *Test:* oracle.
- **Follow-up 3:** Linear time guarantee. *Test:* pathological pattern scale test.
- **Trade-offs:** NFA simulation vs backtracking vs DFA. **Concepts:** automata, parsing, differential testing.
- **After this:** you can match without catastrophic backtracking. **In projects:** search and validation.
- **Required first:** `iv-recursive-descent` (PARSE 11). **Helpful:** `iv-glob` (PARSE 10). **Used again in:** `iv-nfa-anchors-search` (PARSE 21).
- **Cookbook:** similar\* `regex-nfa-engine` (parser, NFA, oracle). **Status:** exists: `y8-regex-nfa-matcher`. **Runner:** gradable today.

#### PARSE 16. Parser combinator library

`iv-parser-combinators` · 90 min · hard · P2 · Tracks: L4

- **New idea:** parsers are values that compose.
- **Core:** Parser as impl Fn(&str) -> Option<(O, &str)> with map, and, or, many.
- **Follow-up 1:** Errors with furthest position. *Test:* tables.
- **Follow-up 2:** Recursion through a lazy combinator. *Test:* nested lists.
- **Follow-up 3:** Rebuild the calculator with it. *Test:* same oracle.
- **Trade-offs:** impl Trait vs Box<dyn>. **Concepts:** closures, generics, lifetimes.
- **After this:** you can build parsers from parts. **In projects:** protocol and DSL code.
- **Required first:** `iv-recursive-descent` (PARSE 11). **Helpful:** `iv-fn-pipeline` (ITER 2). **Used again in:** `iv-peg-packrat` (LANG 7).
- **Cookbook:** similar\* `parser-combinators` (core combinators), similar `hrtb-gats`. **Status:** to be written. **Runner:** gradable today.

#### PARSE 17. Streaming JSON tokenizer

`iv-json-streaming` · 90 min · hard · P2 · Tracks: S9

- **New idea:** a parser that can stop and resume anywhere.
- **Core:** Feed chunks and emit events.
- **Follow-up 1:** Any split point gives the same events. *Test:* all splits of random documents.
- **Follow-up 2:** Bounded memory. *Test:* allocation counting on huge arrays.
- **Follow-up 3:** Path filter that extracts one field without building the tree. *Test:* oracle.
- **Trade-offs:** Resumable state machine. **Concepts:** push parser, state stack.
- **After this:** you can parse data larger than memory. **In projects:** ingest pipelines.
- **Required first:** `iv-json` (PARSE 13). **Helpful:** `iv-length-codec` (BIN 2).
- **Cookbook:** related `json-parser`, related `csv-state-machine`. **Status:** to be written. **Runner:** gradable today.

#### PARSE 18. Parser with error recovery

`iv-error-recovery` · 90 min · hard · P2 · Tracks: S2

- **New idea:** skip to a safe point and keep reporting.
- **Core:** Parse a small statement language, reporting the first error.
- **Follow-up 1:** Panic-mode recovery to report many errors. *Test:* expected error lists.
- **Follow-up 2:** Error nodes in the tree. *Test:* tree still covers the input.
- **Follow-up 3:** No cascades or infinite loops. *Test:* random token soup terminates.
- **Trade-offs:** Synchronisation sets. **Concepts:** recovery, diagnostics.
- **After this:** you can write friendly compilers. **In projects:** IDE tooling.
- **Required first:** `iv-pratt` (PARSE 12). **Helpful:** `iv-diagnostics-renderer` (ERR 1).
- **Cookbook:** similar `toy-language-interpreter` (panic-mode recovery). **Status:** to be written. **Runner:** gradable today.

#### PARSE 19. Statement parser with desugaring

`iv-stmt-parser-desugar` · 60 min · medium · P2 · prep · Tracks: PARSE

- **New idea:** sugar is lowered in the parser to a smaller core tree.
- **Core:** Parse blocks, if, while, for and assignment into a small AST, lowering for while keeping continue correct and rejecting invalid assignment targets.
- **Follow-up 1:** add compound assignment by lowering. *Test:* tree equals the hand-written expansion ; add an unless or repeat form without new AST nodes :: tree dumps and evaluation results.
- **Required first:** `iv-recursive-descent` (PARSE 11), `iv-pratt` (PARSE 12).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

#### PARSE 20. Character class and escape syntax

`iv-regex-syntax` · 45 min · medium · P2 · prep · Tracks: PARSE

- **New idea:** inside brackets the grammar changes, so ranges, dashes and escapes need their own rules.
- **Core:** Parse bracket classes and shorthand escapes into range sets with positioned errors.
- **Follow-up 1:** follow-up 1 shorthand escapes inside classes. *Test:* membership table ; follow-up 2 normalise to sorted disjoint ranges :: seeded characters give the same membership before and after.
- **Required first:** `iv-recursive-descent` (PARSE 11).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

#### PARSE 21. Anchors and unanchored search on an NFA

`iv-nfa-anchors-search` · 90 min · hard · P2 · prep · Tracks: PARSE

- **New idea:** some epsilon edges depend on the position, and search must choose which match to report.
- **Core:** Add start and end anchors as conditional epsilon edges to a given NFA and implement leftmost-longest find.
- **Follow-up 1:** follow-up 1 single-pass search injecting a new start thread per step with start offsets. *Test:* differential against the per-start version ; follow-up 2 word-boundary assertion :: table and differential with a backtracker.
- **Required first:** `iv-regex-nfa` (PARSE 15).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

#### PARSE 22. Backtracking matcher with continuations

`iv-cps-backtracker` · 90 min · hard · P2 · prep · Tracks: PARSE

- **New idea:** passing the rest of the match as a closure makes backtracking fall out of recursion.
- **Core:** Match a small regex AST with dyn Fn continuations, a step budget and a guard against empty loop iterations.
- **Follow-up 1:** follow-up 1 find all end positions and report the longest. *Test:* table including nested stars ; follow-up 2 lazy quantifiers with first-match order :: expected spans on fixed cases.
- **Required first:** `iv-recursive-descent` (PARSE 11).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

### LANG: From parser to a language

Give the parsed tree meaning, then compile it. *Core series, 16 parts, about 26 h. Ends in `cap-lox`.* Alongside it: `iv-test-differential` (TEST 6).

#### LANG 1. S-expression reader and evaluator

`iv-sexpr-lisp` · 90 min · medium · P2 · Tracks: S7

- **New idea:** evaluation with environments and closures.
- **Core:** Parse s-expressions and evaluate arithmetic.
- **Follow-up 1:** define, if, lambda with closures. *Test:* programs with expected results.
- **Follow-up 2:** Tail calls without stack growth. *Test:* deep recursion.
- **Follow-up 3:** Errors with spans. *Test:* table.
- **Trade-offs:** Rc environments. **Concepts:** parsing, environments, closures.
- **After this:** you can embed a scripting language. **In projects:** rule engines.
- **Required first:** `iv-tokenizer-borrowed` (PARSE 2). **Used again in:** `iv-tree-interpreter` (LANG 4), `iv-formatter` (LANG 8), `iv-interp-object-model` (LANG 14).
- **Cookbook:** related `cps-closure-conversion-lambda-lifting`, related `lambda-calculus-toolkit`. **Status:** to be written. **Runner:** gradable today.

#### LANG 2. Optimisation passes on a small IR

`iv-constant-fold` · 45 min · medium · P2 · Tracks: L7

- **New idea:** rewrite a tree without changing its meaning.
- **Core:** Constant folding on an expression tree.
- **Follow-up 1:** Dead code and unused lets. *Test:* semantics preserved on random programs.
- **Follow-up 2:** Algebraic identities with overflow care. *Test:* differential.
- **Follow-up 3:** Run to fixpoint. *Test:* idempotence.
- **Trade-offs:** Pass ordering. **Concepts:** tree rewriting, differential testing.
- **After this:** you can write optimisation passes. **In projects:** query and build optimisers.
- **Required first:** `iv-pratt` (PARSE 12). **Helpful:** `iv-alg-ast-walk` (REC 2). **Used again in:** `iv-dataflow` (LANG 6), `iv-planner-rules` (QUERY 9).
- **Cookbook:** similar `bytecode-vm` (folding). **Status:** to be written. **Runner:** gradable today.

#### LANG 3. Type checker for a small language

`iv-typecheck-simple` · 60 min · medium · P2 · Tracks: L7

- **New idea:** reject programs before running them.
- **Core:** Check ints, bools, lets and ifs with errors.
- **Follow-up 1:** Function types. *Test:* corpus.
- **Follow-up 2:** Collect many errors without cascades. *Test:* expected lists.
- **Follow-up 3:** Bidirectional annotations. *Test:* corpus.
- **Trade-offs:** Checking vs inference. **Concepts:** type rules, diagnostics.
- **After this:** you can check DSLs. **In projects:** config and workflow languages.
- **Required first:** `iv-pratt` (PARSE 12). **Helpful:** `iv-diagnostics-renderer` (ERR 1). **Used again in:** `iv-hindley-milner` (LANG 9), `iv-unification` (LANG 10).
- **Cookbook:** similar `system-f-bidirectional-typing`. **Status:** to be written. **Runner:** gradable today.

#### LANG 4. Tree-walking interpreter

`iv-tree-interpreter` · 120 min · hard · P1 · Tracks: S7

- **New idea:** resolve scopes, then walk the tree.
- **Core:** Variables, if, while, print over a given AST.
- **Follow-up 1:** Functions and closures. *Test:* program corpus.
- **Follow-up 2:** Static resolver for scopes. *Test:* shadowing cases.
- **Follow-up 3:** Runtime errors with traces and a step limit. *Test:* golden.
- **Trade-offs:** Rc environments and cycles. **Concepts:** environments, closures.
- **After this:** you can ship an interpreter. **In projects:** automation tools.
- **Required first:** `iv-pratt` (PARSE 12). **Helpful:** `iv-sexpr-lisp` (LANG 1), `iv-graph-ownership` (OWN 6). **Used again in:** `iv-bytecode-vm` (LANG 5).
- **Cookbook:** similar `toy-language-interpreter`. **Status:** to be written. **Runner:** gradable today.

#### LANG 5. Stack bytecode compiler and VM

`iv-bytecode-vm` · 120 min · hard · P1 · Tracks: S3

- **New idea:** compile to instructions for a stack machine.
- **Core:** Compile arithmetic AST to ops and run them.
- **Follow-up 1:** Locals, jumps for if and while. *Test:* differential against a tree evaluator.
- **Follow-up 2:** Calls with frames. *Test:* recursion corpus.
- **Follow-up 3:** Verifier rejecting bad bytecode. *Test:* fuzzed programs never panic.
- **Trade-offs:** Stack vs register VM. **Concepts:** code generation, dispatch.
- **After this:** you can speed up evaluation. **In projects:** rule engines and VMs.
- **Required first:** `iv-tree-interpreter` (LANG 4). **Used again in:** `iv-wasm-interp` (LANG 11), `iv-vm-local-slots` (LANG 15), `iv-sbx-fuel` (SBX 5).
- **Cookbook:** similar\* `bytecode-vm` (arithmetic compiler and VM). **Status:** to be written (partly practised by `f2-eight-byte-instructions`). **Runner:** gradable today.

#### LANG 6. Dataflow analysis framework

`iv-dataflow` · 90 min · medium · P3 · Tracks: D

- **New idea:** iterate facts over a graph until they stop changing.
- **Core:** Worklist solver for liveness.
- **Follow-up 1:** Reaching definitions and available expressions. *Test:* naive fixpoint oracle.
- **Follow-up 2:** Constant propagation lattice. *Test:* soundness against traces.
- **Follow-up 3:** Dominators. *Test:* brute force.
- **Trade-offs:** Lattices. **Concepts:** fixpoints.
- **After this:** you can write static analyses. **In projects:** linters.
- **Required first:** `iv-constant-fold` (LANG 2). **Helpful:** `iv-alg-reach` (GRAPH 2). **Used again in:** `iv-ssa-regalloc` (LANG 12).
- **Cookbook:** similar\* `dataflow-analysis-framework`. **Status:** to be written. **Runner:** gradable today.

#### LANG 7. PEG parser with memoisation

`iv-peg-packrat` · 90 min · medium · P3 · Tracks: D

- **New idea:** memoised backtracking stays linear.
- **Core:** Interpret a PEG grammar.
- **Follow-up 1:** Packrat memo. *Test:* linear step bound.
- **Follow-up 2:** Left-recursion detection. *Test:* error.
- **Follow-up 3:** Parse tree output. *Test:* golden.
- **Trade-offs:** PEG vs CFG. **Concepts:** memoisation.
- **After this:** you can parse ambiguous-looking grammars. **In projects:** tools.
- **Required first:** `iv-parser-combinators` (PARSE 16). **Helpful:** `iv-memoize` (ITER 3).
- **Cookbook:** similar\* `parser-generators-lr-peg-earley`. **Status:** to be written. **Runner:** gradable today.

#### LANG 8. Pretty printer

`iv-formatter` · 90 min · medium · P3 · Tracks: D

- **New idea:** layout decided by whether a group fits.
- **Core:** Wadler-style group, line and nest with a width.
- **Follow-up 1:** Width bound. *Test:* property.
- **Follow-up 2:** Format an s-expression language idempotently. *Test:* round trip.
- **Follow-up 3:** Comments preserved. *Test:* corpus.
- **Trade-offs:** Greedy fits. **Concepts:** document algebra.
- **After this:** you can write code formatters. **In projects:** developer tools.
- **Required first:** `iv-sexpr-lisp` (LANG 1).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

#### LANG 9. Hindley-Milner inference

`iv-hindley-milner` · 120 min · hard · P2 · Tracks: D

- **New idea:** infer types by solving equations.
- **Core:** Unification and inference for lambdas.
- **Follow-up 1:** let-polymorphism with levels. *Test:* corpus.
- **Follow-up 2:** Occurs check and good errors. *Test:* tables.
- **Follow-up 3:** Random well-typed terms infer. *Test:* property.
- **Trade-offs:** Value restriction. **Concepts:** unification, union-find.
- **After this:** you can understand inference. **In projects:** typed DSLs.
- **Required first:** `iv-typecheck-simple` (LANG 3).
- <details><summary>Links hidden until asked</summary>helpful: `iv-union-find` (COLL 7)</details>
- **Cookbook:** similar\* `hindley-milner-inference`. **Status:** to be written. **Runner:** gradable today.

#### LANG 10. Unification and a tiny Prolog

`iv-unification` · 120 min · hard · P3 · Tracks: D

- **New idea:** search with substitutions and undo.
- **Core:** unify terms with occurs check.
- **Follow-up 1:** Backtracking resolution. *Test:* known queries.
- **Follow-up 2:** Cut or negation. *Test:* corpus.
- **Follow-up 3:** Iterative deepening. *Test:* terminates.
- **Trade-offs:** Search order. **Concepts:** unification, backtracking.
- **After this:** you can build logic engines. **In projects:** policy and query engines.
- **Required first:** `iv-typecheck-simple` (LANG 3).
- <details><summary>Links hidden until asked</summary>helpful: `iv-union-find` (COLL 7)</details>
- **Cookbook:** similar\* `term-rewriting-unification-logic-programming`. **Status:** to be written. **Runner:** gradable today.

#### LANG 11. WebAssembly integer interpreter

`iv-wasm-interp` · 120 min · hard · P3 · Tracks: Y6

- **New idea:** decode, validate, then execute a real format.
- **Core:** Decode and run i32 functions.
- **Follow-up 1:** Structured control flow. *Test:* corpus.
- **Follow-up 2:** Calls and traps. *Test:* corpus.
- **Follow-up 3:** Validation. *Test:* fuzz never panics.
- **Trade-offs:** Validation before execution. **Concepts:** binary decoding, stack machine.
- **After this:** you can run untrusted modules. **In projects:** plugin hosts.
- **Required first:** `iv-bytecode-vm` (LANG 5). **Helpful:** `iv-varint-codec` (BIN 1).
- **Cookbook:** similar\* `wasm-binary-interpreter`. **Status:** exists: `y6-wasm-binary-decoder`, `y6-wasm-stack-interpreter`. **Runner:** gradable today.

#### LANG 12. Liveness and register allocation

`iv-ssa-regalloc` · 120 min · hard · P3 · Tracks: D

- **New idea:** values that live at once cannot share a register.
- **Core:** Liveness over a CFG.
- **Follow-up 1:** Interference graph colouring with spills. *Test:* interpreter equivalence.
- **Follow-up 2:** Linear scan. *Test:* equivalence.
- **Follow-up 3:** Parallel copy sequencing. *Test:* cycles.
- **Trade-offs:** Quality vs time. **Concepts:** dataflow, graph colouring.
- **After this:** you can read compiler back ends. **In projects:** compilers.
- **Required first:** `iv-dataflow` (LANG 6).
- <details><summary>Links hidden until asked</summary>helpful: `iv-alg-order` (GRAPH 4)</details>
- **Cookbook:** similar\* `ssa-register-allocation`. **Status:** to be written. **Runner:** gradable today.

#### LANG 13. Call frames and stack traces

`iv-interp-call-frames` · 90 min · hard · P2 · prep · Tracks: LANG

- **New idea:** a call is an explicit frame that gives traces, return points and a depth limit.
- **Core:** Run a tiny language of functions on an explicit frame stack with a base slot and saved position, producing exact traces and a stack overflow error.
- **Follow-up 1:** add a depth-limited recursive evaluator run on a thread with a chosen stack size. *Test:* deep input succeeds up to the limit and errors one past it ; add per-instruction line tables :: exact traces through three frames.
- **Required first:** `iv-recursive-descent` (PARSE 11). **Used again in:** `iv-vm-upvalues` (LANG 16).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

#### LANG 14. Objects, method lookup and bound methods

`iv-interp-object-model` · 90 min · hard · P2 · prep · Tracks: LANG

- **New idea:** a method read from an object is a function closed over that object, found by walking the class chain.
- **Core:** Implement classes with fields, single inheritance, bound methods, an initializer and super calls over a small hand-built AST.
- **Follow-up 1:** super in a three-level chain starts above the defining class, not the receiver class. *Test:* exact result strings ; a stored bound method keeps its receiver after reassignment :: assertions on field changes.
- **Required first:** `iv-sexpr-lisp` (LANG 1).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

#### LANG 15. Local variables as stack slots

`iv-vm-local-slots` · 60 min · medium · P2 · prep · Tracks: LANG

- **New idea:** the compiler mirrors the runtime stack so each local is an index known before running.
- **Core:** Compile nested blocks with shadowing to slot-indexed instructions and emit the right pops at scope exit.
- **Follow-up 1:** add break and continue that pop inner locals before jumping. *Test:* stack height after each loop equals height before ; report the maximum stack depth at compile time :: equals the observed maximum.
- **Required first:** `iv-bytecode-vm` (LANG 5). **Used again in:** `iv-vm-upvalues` (LANG 16).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

#### LANG 16. Closures with upvalues

`iv-vm-upvalues` · 120 min · hard · P2 · prep · Tracks: LANG

- **New idea:** a captured variable lives on the stack while its scope is alive and moves to the heap when it ends.
- **Core:** Add capture resolution through enclosing functions and open and closed upvalues to a small stack VM.
- **Follow-up 1:** two closures share one variable before and after closing. *Test:* exact output ; fresh capture per loop iteration, also when leaving by break :: closures return distinct values.
- **Required first:** `iv-vm-local-slots` (LANG 15), `iv-interp-call-frames` (LANG 13).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

### BIN: Binary formats and wire protocols

Bytes in, structure out, never a panic. *Core series, 13 parts, about 15 h. Ends in `cap-queue`.* Alongside it: `iv-fuzz-harness` (TEST 5).

#### BIN 1. Varints and a TLV format

`iv-varint-codec` · 45 min · easy · P1 · Tracks: F5

- **New idea:** variable-length integers and tagged records.
- **Core:** LEB128 and zigzag encode and decode with errors.
- **Follow-up 1:** Length-delimited records skipping unknown tags. *Test:* forward-compatibility tests.
- **Follow-up 2:** CRC per record. *Test:* corruption detected.
- **Follow-up 3:** Zero-copy decode. *Test:* borrowed payloads.
- **Trade-offs:** Overflow and truncation. **Concepts:** binary formats, bounds checks.
- **After this:** you can design compact formats. **In projects:** storage and RPC.
- **Required first:** nothing. **Used again in:** `iv-wasm-interp` (LANG 11), `iv-length-codec` (BIN 2), `iv-protobuf-wire` (BIN 5), `iv-gorilla` (BIN 7), `iv-lz77` (BIN 8), `iv-dns-codec` (BIN 10), `iv-serde-format` (BIN 11), `iv-row-serialization` (STORE 1), `iv-wal` (STORE 2), `iv-columnar-encodings` (STORE 10), `iv-posting-compression` (SEARCH 2).
- **Cookbook:** similar\* `binary-format-parsing` (bounds-checked reader and TLV), similar `record-batch-codec`. **Status:** to be written (partly practised by `f5-varint-codec`). **Runner:** gradable today.

#### BIN 2. Length-prefixed frame codec

`iv-length-codec` · 45 min · medium · P1 · Tracks: B4

- **New idea:** buffer until a whole frame has arrived.
- **Core:** feed bytes, pop complete frames.
- **Follow-up 1:** Max length before buffering. *Test:* hostile length.
- **Follow-up 2:** Zero-copy frames where possible. *Test:* allocation counting.
- **Follow-up 3:** Encoder and round trip over random chunking. *Test:* property.
- **Trade-offs:** Buffer compaction. **Concepts:** incremental parsing, buffers.
- **After this:** you can frame any stream. **In projects:** TCP protocols.
- **Required first:** `iv-varint-codec` (BIN 1). **Helpful:** `iv-ring-deque` (COLL 2). **Used again in:** `iv-cancel-safety` (ASYNC 6), `iv-json-streaming` (PARSE 17), `iv-resp` (BIN 4), `iv-sans-io` (NET 1), `iv-websocket-frames` (NET 4), `iv-tcp-echo-framed` (NET 5), `iv-http-request` (NET 6), `iv-tokio-tcp-conn` (NET 9).
- **Cookbook:** related `tcp-framed-server`, related `bytes-buf-buf-mut-framing`. **Status:** exists: `b4-length-prefixed-codec`. **Runner:** gradable today.

#### BIN 3. Match a JSON wire format with serde

`iv-serde-wire` · 60 min · medium · P1 · Tracks: F7

- **New idea:** attributes map a foreign JSON shape to types.
- **Core:** Derive types for a given payload set.
- **Follow-up 1:** Tagged enums and defaults. *Test:* golden round trip.
- **Follow-up 2:** Unknown fields policy and versioning. *Test:* old and new payloads.
- **Follow-up 3:** A custom (de)serialize for one field. *Test:* errors.
- **Trade-offs:** Strict vs lenient. **Concepts:** serde attributes.
- **After this:** you can integrate with APIs you do not control. **In projects:** any service client.
- **Required first:** nothing. **Used again in:** `iv-serde-format` (BIN 11), `iv-lin-upcasting` (LIN 6).
- **Cookbook:** similar `serde-attributes-enum-representations`, similar `serde-custom`. **Status:** staged draft: `f7-wire-format-json`. **Runner:** ok (serde, serde_json).

#### BIN 4. RESP (Redis protocol) parser

`iv-resp` · 60 min · medium · P2 · Tracks: B4

- **New idea:** an incremental parser for nested frames.
- **Core:** Parse and encode all RESP2 types incrementally.
- **Follow-up 1:** Inline commands. *Test:* tables.
- **Follow-up 2:** Limits checked before allocation. *Test:* huge declared lengths.
- **Follow-up 3:** Pipelined commands. *Test:* every split point.
- **Trade-offs:** Recursion depth. **Concepts:** incremental recursive parser.
- **After this:** you can speak a real protocol. **In projects:** Redis-like servers.
- **Required first:** `iv-length-codec` (BIN 2). **Helpful:** `iv-recursive-descent` (PARSE 11).
- **Cookbook:** similar\* `resp-protocol` (incremental parser and limits). **Status:** to be written. **Runner:** gradable today.

#### BIN 5. Protocol Buffers wire format

`iv-protobuf-wire` · 60 min · medium · P3 · Tracks: F7

- **New idea:** tags make formats forward compatible.
- **Core:** Encode and decode varint, fixed and length-delimited fields.
- **Follow-up 1:** Unknown fields preserved. *Test:* round trip across schema versions.
- **Follow-up 2:** Packed repeated and nested messages. *Test:* random messages.
- **Follow-up 3:** Merge semantics. *Test:* law tests.
- **Trade-offs:** Compatibility rules. **Concepts:** tag-wire encoding.
- **After this:** you can evolve schemas safely. **In projects:** RPC.
- **Required first:** `iv-varint-codec` (BIN 1).
- **Cookbook:** similar\* `protobuf-grpc-wire-format`. **Status:** to be written. **Runner:** gradable today.

#### BIN 6. Packet headers and checksum

`iv-packet-parse` · 60 min · medium · P3 · Tracks: B4

- **New idea:** typed views over layered headers.
- **Core:** Parse Ethernet, IPv4 and TCP headers as views.
- **Follow-up 1:** Internet checksum verify and build. *Test:* known vectors.
- **Follow-up 2:** IPv4 reassembly. *Test:* shuffled fragments.
- **Follow-up 3:** Flow table. *Test:* counts.
- **Trade-offs:** Alignment and endianness. **Concepts:** views over bytes.
- **After this:** you can inspect network data. **In projects:** proxies and monitors.
- **Required first:** `iv-layout-quiz-codec` (UNSAFE 1). **Used again in:** `iv-dns-codec` (BIN 10).
- **Cookbook:** similar\* `packet-parsing-checksums`. **Status:** to be written. **Runner:** gradable today.

#### BIN 7. Time-series compression

`iv-gorilla` · 60 min · medium · P3 · Tracks: F7

- **New idea:** encode differences at bit granularity.
- **Core:** Delta-of-delta timestamps.
- **Follow-up 1:** XOR floats. *Test:* bit-exact round trip including NaN.
- **Follow-up 2:** Chunking and seek. *Test:* oracle.
- **Follow-up 3:** simple8b integers. *Test:* round trip.
- **Trade-offs:** Data shape assumptions. **Concepts:** bit streams.
- **After this:** you can store time series. **In projects:** metrics databases.
- **Required first:** `iv-varint-codec` (BIN 1). **Helpful:** `iv-alg-bit-fields` (BIT 2).
- **Cookbook:** similar\* `timeseries-gorilla`. **Status:** to be written. **Runner:** gradable today.

#### BIN 8. LZ77 compression

`iv-lz77` · 90 min · hard · P3 · Tracks: F5

- **New idea:** replace repeats with back-references.
- **Core:** Find back-references with a window and decode them.
- **Follow-up 1:** Hash-chain match finder. *Test:* round trip on random and repetitive data, work bound.
- **Follow-up 2:** Overlapping copies. *Test:* run-length cases.
- **Follow-up 3:** Decoder safe on hostile input. *Test:* fuzz-style, never panics.
- **Trade-offs:** Ratio vs speed. **Concepts:** LZ77, hash chains.
- **After this:** you can explain compression. **In projects:** storage and transport.
- **Required first:** `iv-varint-codec` (BIN 1).
- **Cookbook:** similar\* `compression-lz77-huffman` (its LZ77 part). **Status:** to be written. **Runner:** gradable today.

#### BIN 9. Canonical Huffman coding

`iv-huffman` · 90 min · hard · P3 · Tracks: F5

- **New idea:** short codes for frequent symbols.
- **Core:** Build code lengths from frequencies, encode and decode.
- **Follow-up 1:** Canonical codes from lengths only. *Test:* round trip with header.
- **Follow-up 2:** Single-symbol and empty inputs. *Test:* tables.
- **Follow-up 3:** Bit reader on truncated input. *Test:* errors, no panic.
- **Trade-offs:** Table-driven decoding. **Concepts:** entropy coding, prefix codes.
- **After this:** you can build entropy coders. **In projects:** compression formats.
- <details><summary>Links hidden until asked</summary>required first: `iv-binary-heap` (COLL 4)</details>
- **Cookbook:** similar\* `compression-lz77-huffman` (its Huffman part). **Status:** to be written. **Runner:** gradable today.

#### BIN 10. DNS message codec

`iv-dns-codec` · 120 min · hard · P3 · Tracks: B4

- **New idea:** pointers inside untrusted data need loop limits.
- **Core:** Decode header, questions and A records.
- **Follow-up 1:** Name compression with loop detection. *Test:* pointer attacks.
- **Follow-up 2:** Encoder with compression. *Test:* round trip.
- **Follow-up 3:** Fuzz-style robustness. *Test:* mutated packets never panic.
- **Trade-offs:** Offsets in untrusted data. **Concepts:** binary parsing, safety.
- **After this:** you can parse hostile binary formats. **In projects:** resolvers.
- **Required first:** `iv-varint-codec` (BIN 1). **Helpful:** `iv-packet-parse` (BIN 6).
- **Cookbook:** similar\* `dns-message-codec`. **Status:** to be written. **Runner:** gradable today.

#### BIN 11. A tiny serde data format

`iv-serde-format` · 120 min · hard · P3 · Tracks: F7

- **New idea:** the serde data model from the other side.
- **Core:** Serializer for a compact format.
- **Follow-up 1:** Deserializer. *Test:* round trip of random structs.
- **Follow-up 2:** Enums and maps. *Test:* round trip.
- **Follow-up 3:** Errors with offsets. *Test:* truncated input.
- **Trade-offs:** The data model. **Concepts:** Serializer, Visitor.
- **After this:** you can add a format to the ecosystem. **In projects:** storage layers.
- **Required first:** `iv-serde-wire` (BIN 3). **Helpful:** `iv-varint-codec` (BIN 1).
- **Cookbook:** similar\* `serde-custom` (its complete custom format). **Status:** to be written. **Runner:** gradable today.

#### BIN 12. CRC-32 and Adler-32

`iv-checksums` · 45 min · medium · P2 · prep · Tracks: BIN

- **New idea:** a checksum is a small running state that detects damaged bytes.
- **Core:** Implement bitwise CRC-32 (IEEE, reflected) and Adler-32 with an incremental update interface.
- **Follow-up 1:** table-driven CRC. *Test:* equals the bitwise version on seeded buffers split at random points ; detect corruption :: every single-bit flip changes the CRC, and show that CRC of empty input is 0 so zeros can look like a valid empty record.
- **Required first:** nothing. **Used again in:** `iv-png-container` (BIN 13), `iv-redo-journal` (STORE 16).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

#### BIN 13. Write a PNG by hand

`iv-png-container` · 60 min · medium · P2 · prep · Tracks: BIN

- **New idea:** a file format is nested length-delimited, checksummed containers.
- **Core:** Encode RGB pixels as a PNG using filter 0 and stored deflate blocks, converting linear floats to sRGB bytes.
- **Follow-up 1:** split blocks over 65535 bytes. *Test:* a test decoder reassembles a large image ; reject bad dimensions and NaN pixels :: error table and exact bytes for clamped values.
- **Required first:** `iv-checksums` (BIN 12), `iv-layout-quiz-codec` (UNSAFE 1).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

### IO: Files, processes and standard I/O

Read, write and run things that can fail half way. *Core series, 8 parts, about 7 h. Ends in `cap-fs`.* Alongside it: `iv-test-failpoints` (TEST 7).

#### IO 1. Streaming log analyser

`iv-log-analyzer` · 45 min · easy · P2 · Tracks: S9 S4

- **New idea:** aggregate a stream in bounded memory.
- **Core:** Count status codes and top paths from a reader.
- **Follow-up 1:** Percentiles in bounded memory. *Test:* error bound.
- **Follow-up 2:** Malformed lines reported with numbers. *Test:* tables.
- **Follow-up 3:** Merge several time-ordered logs. *Test:* oracle.
- **Trade-offs:** Memory vs accuracy. **Concepts:** streaming, aggregation.
- **After this:** you can write log tools. **In projects:** operations.
- **Required first:** nothing.
- **Cookbook:** similar `std-io-stdio-terminal-text-processing`. **Status:** to be written (partly practised by `s2-word-frequency`). **Runner:** gradable today.

#### IO 2. Bounded line reader

`iv-bounded-line-reader` · 60 min · medium · P1 · Tracks: S9

- **New idea:** fill_buf and consume with a length limit.
- **Core:** Read lines from any BufRead with a maximum length.
- **Follow-up 1:** Resynchronise after a long line. *Test:* following lines intact.
- **Follow-up 2:** Retry Interrupted and handle short reads. *Test:* hostile mock readers.
- **Follow-up 3:** No allocation per line. *Test:* allocation counting.
- **Trade-offs:** read_until pitfalls. **Concepts:** BufRead, fill_buf, consume.
- **After this:** you can read untrusted streams. **In projects:** servers.
- **Required first:** nothing. **Used again in:** `iv-bufreader` (IO 3).
- **Cookbook:** similar `buffered-io-internals`, related `std-io-read-write-seek-traits`. **Status:** staged draft: `s9-bounded-line-reader`. **Runner:** gradable today.

#### IO 3. BufReader and BufWriter from scratch

`iv-bufreader` · 60 min · medium · P1 · Tracks: S9

- **New idea:** buffering trades copies for system calls.
- **Core:** MyBufReader implementing Read and BufRead.
- **Follow-up 1:** MyBufWriter with flush on drop. *Test:* syscall-counting mocks.
- **Follow-up 2:** Large reads bypass the buffer. *Test:* counts.
- **Follow-up 3:** LineWriter. *Test:* flush points.
- **Trade-offs:** Flush errors in Drop. **Concepts:** Read, Write, buffering.
- **After this:** you can explain I/O performance. **In projects:** every program.
- **Required first:** `iv-bounded-line-reader` (IO 2). **Used again in:** `iv-io-adapters` (IO 4).
- **Cookbook:** similar\* `buffered-io-internals`. **Status:** to be written. **Runner:** gradable today.

#### IO 4. Read and Write adapters

`iv-io-adapters` · 45 min · medium · P2 · Tracks: S9

- **New idea:** wrapping Read and Write composes behaviour.
- **Core:** CountingWriter, LimitReader and TeeReader.
- **Follow-up 1:** write_all and short writes. *Test:* hostile mocks.
- **Follow-up 2:** Seekable window over a file-like. *Test:* Cursor oracle.
- **Follow-up 3:** Chain several. *Test:* golden.
- **Trade-offs:** Adapter composition. **Concepts:** io traits, generics.
- **After this:** you can meter and limit streams. **In projects:** proxies and uploads.
- **Required first:** `iv-bufreader` (IO 3). **Helpful:** `iv-iter-adapters` (ITER 1). **Used again in:** `iv-atomic-file` (IO 5).
- **Cookbook:** related `std-io-read-write-seek-traits`. **Status:** to be written. **Runner:** gradable today.

#### IO 5. Crash-safe file replace

`iv-atomic-file` · 60 min · medium · P2 · Tracks: S9

- **New idea:** rename makes replacement all or nothing.
- **Core:** write to temp, sync, rename.
- **Follow-up 1:** Directory sync and cleanup of stale temps. *Test:* injected failures through a Fs trait.
- **Follow-up 2:** Fault at every step leaves old or new. *Test:* simulated file system.
- **Follow-up 3:** Lock file. *Test:* one winner among threads.
- **Trade-offs:** What rename guarantees. **Concepts:** atomic rename, fault injection.
- **After this:** you can save files that survive crashes. **In projects:** config and state files.
- **Required first:** `iv-io-adapters` (IO 4). **Used again in:** `iv-online-log-rewrite` (STORE 19), `iv-crash-ordered-files` (STORE 20).
- **Cookbook:** similar\* `file-locking-atomic-replace`. **Status:** to be written. **Runner:** needs a writable temp directory; the sandbox mounts /tmp as tmpfs, not verified here.

#### IO 6. Directory walker with filters

`iv-dir-walker` · 60 min · medium · P3 · Tracks: S9

- **New idea:** walk a tree lazily and survive errors.
- **Core:** Recursive walk with glob filters.
- **Follow-up 1:** Symlink loops. *Test:* fixtures.
- **Follow-up 2:** Errors collected, walk continues. *Test:* permissions.
- **Follow-up 3:** Lazy iterator. *Test:* early stop.
- **Trade-offs:** Recursion vs stack. **Concepts:** fs, Path, iterator.
- **After this:** you can write build and cleanup tools. **In projects:** developer tools.
- **Required first:** `iv-flatten-tree-iter` (ITER 5). **Helpful:** `iv-glob` (PARSE 10).
- **Cookbook:** similar `std-fs-file-operations-usecases`. **Status:** to be written. **Runner:** needs a writable temp directory; not verified in the sandbox.

#### IO 7. Run a pipeline of child processes

`iv-process-pipeline` · 60 min · medium · P2 · Tracks: S10

- **New idea:** drain pipes or deadlock.
- **Core:** Spawn two commands connected by a pipe and collect output.
- **Follow-up 1:** Avoid the full-pipe deadlock. *Test:* large input.
- **Follow-up 2:** Timeout and kill. *Test:* generous bounds.
- **Follow-up 3:** Exit status and signals. *Test:* tables.
- **Trade-offs:** Zombies. **Concepts:** Command, pipes, threads.
- **After this:** you can run external tools safely. **In projects:** job runners.
- **Required first:** `iv-blocking-queue` (CONC 2). **Used again in:** `iv-sbx-supervisor` (SBX 6).
- **Cookbook:** similar\* `std-process-command-pipelines`, similar `process-pipes-signals`. **Status:** to be written. **Runner:** needs process spawning and known binaries in the runner image; not verified.

#### IO 8. A file as an array of blocks

`iv-positional-block-io` · 45 min · medium · P2 · prep · Tracks: IO

- **New idea:** positional reads and writes with explicit sync turn one file into a block device.
- **Core:** Implement a block device trait over memory and over a file using positional exact reads and writes with bounds checks.
- **Follow-up 1:** add a counting wrapper device. *Test:* exact operation log ; reject a file whose length is not a whole number of blocks :: typed error on a crafted file.
- **Required first:** nothing. **Used again in:** `iv-disk-fault-model` (TEST 10).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

### OS: Operating-system ideas

Simulate what the kernel does for you. *Core series, 8 parts, about 10 h. Ends in `cap-fs`.* Alongside it: `iv-mon-metrics` (MON 2).

#### OS 1. CPU scheduling simulator

`iv-cpu-scheduler` · 60 min · medium · P1 · Tracks: D

- **New idea:** scheduling policies trade latency for fairness.
- **Core:** FCFS and round robin with a trace.
- **Follow-up 1:** SJF and SRTF. *Test:* brute force on small sets.
- **Follow-up 2:** Priority with ageing. *Test:* starvation bound.
- **Follow-up 3:** MLFQ. *Test:* scripted traces.
- **Trade-offs:** Fairness vs latency. **Concepts:** simulation, queues.
- **After this:** you can choose a scheduler. **In projects:** job systems.
- **Required first:** nothing. **Helpful:** `iv-job-scheduler` (CONC 15).
- **Cookbook:** similar\* `scheduler-policies`. **Status:** to be written. **Runner:** gradable today.

#### OS 2. Page replacement simulator

`iv-page-replacement` · 45 min · easy · P2 · Tracks: D

- **New idea:** replacement policy decides the fault count.
- **Core:** FIFO and LRU fault counts.
- **Follow-up 1:** CLOCK and Belady optimal. *Test:* OPT never worse.
- **Follow-up 2:** Belady anomaly reproduced. *Test:* known trace.
- **Follow-up 3:** TLB in front. *Test:* hit counts.
- **Trade-offs:** Stack algorithms. **Concepts:** replacement, simulation.
- **After this:** you can compare eviction fairly. **In projects:** caches.
- **Required first:** `iv-lru` (CACHE 1). **Used again in:** `iv-page-table` (OS 4).
- **Cookbook:** similar\* `virtual-memory-sim`. **Status:** to be written. **Runner:** gradable today.

#### OS 3. Disk head scheduling and RAID parity

`iv-disk-sched-raid` · 45 min · easy · P3 · Tracks: D

- **New idea:** parity rebuilds lost data.
- **Core:** SCAN and C-LOOK order.
- **Follow-up 1:** RAID 0 and 5 mapping. *Test:* round trip.
- **Follow-up 2:** Rebuild a lost disk from parity. *Test:* equality.
- **Follow-up 3:** Scrub with checksums. *Test:* bit flips found.
- **Trade-offs:** Throughput vs fairness. **Concepts:** simulation, XOR.
- **After this:** you can explain storage redundancy. **In projects:** storage operations.
- **Required first:** nothing. **Helpful:** `iv-alg-bit-xor` (BIT 4).
- **Cookbook:** similar\* `disk-scheduling-raid`. **Status:** to be written. **Runner:** gradable today.

#### OS 4. Virtual address translation

`iv-page-table` · 60 min · medium · P3 · Tracks: F3

- **New idea:** a radix tree translates addresses.
- **Core:** Multi-level page table with map and translate.
- **Follow-up 1:** Permissions and faults. *Test:* tables.
- **Follow-up 2:** Demand paging with eviction. *Test:* data survives.
- **Follow-up 3:** TLB invalidation. *Test:* stale-entry test.
- **Trade-offs:** Level count. **Concepts:** radix tree, bits.
- **After this:** you can understand virtual memory. **In projects:** systems debugging.
- **Required first:** `iv-page-replacement` (OS 2).
- **Cookbook:** similar\* `virtual-memory-sim`. **Status:** to be written. **Runner:** gradable today.

#### OS 5. Tiny block file system

`iv-fs-blocks` · 120 min · hard · P3 · Tracks: S9

- **New idea:** files are block lists tracked by bitmaps.
- **Core:** Inodes, bitmap and files over fixed blocks.
- **Follow-up 1:** Directories and paths. *Test:* model comparison.
- **Follow-up 2:** Indirect blocks. *Test:* large files.
- **Follow-up 3:** fsck. *Test:* injected corruption found.
- **Trade-offs:** Metadata ordering. **Concepts:** inodes, allocation.
- **After this:** you can explain filesystems. **In projects:** storage tools.
- **Required first:** `iv-slotted-page` (STORE 3). **Helpful:** `iv-buddy` (MEM 4). **Used again in:** `iv-inode-block-map` (OS 7).
- **Cookbook:** similar\* `toy-filesystem`. **Status:** to be written. **Runner:** ok (in-memory block device); cut: one directory level, no journal.

#### OS 6. Submission and completion rings

`iv-io-uring-model` · 120 min · hard · P3 · Tracks: C3

- **New idea:** submit in batches, complete out of order.
- **Core:** SQ and CQ rings with a fake kernel.
- **Follow-up 1:** Linked entries and batching. *Test:* ordering.
- **Follow-up 2:** Futures over completions. *Test:* manual executor.
- **Follow-up 3:** Backpressure when CQ is full. *Test:* no loss.
- **Trade-offs:** Syscall batching. **Concepts:** SPSC rings, async integration.
- **After this:** you can reason about completion-based I/O. **In projects:** runtimes.
- **Required first:** `iv-spsc` (ATOM 5). **Helpful:** `iv-executor` (ASYNC 2).
- **Cookbook:** similar\* `io-uring-model`. **Status:** to be written. **Runner:** gradable today.

#### OS 7. Inodes and indirect blocks

`iv-inode-block-map` · 90 min · hard · P2 · prep · Tracks: OS

- **New idea:** a file offset maps to a disk block through a small pointer tree that may have holes.
- **Core:** With tiny block and fan-out sizes, implement lookup and allocate through direct, single and double indirect pointers.
- **Follow-up 1:** free everything past a new length including empty pointer blocks. *Test:* free count returns exactly ; sparse read and partial-block write :: compare with a byte-vector model.
- **Required first:** `iv-fs-blocks` (OS 5).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

#### OS 8. Names, inodes and link counts

`iv-unix-namespace` · 90 min · hard · P2 · prep · Tracks: OS

- **New idea:** a name is only a reference to an inode, and every namespace change must keep the counts right.
- **Core:** Over an in-memory inode table with directory maps, implement create, mkdir, link, unlink and rmdir with exact link counts.
- **Follow-up 1:** rename with all replace-target rules and the move-into-own-subtree refusal. *Test:* case table with exact errors ; invariant checker that recounts references :: random operations keep counts equal.
- **Required first:** `iv-file-system-tree` (PAT 11).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

### NET: Networking: bytes to a server

From a lossy channel to a server that sheds load. *Core series, 13 parts, about 20 h. Ends in `cap-httpserver`.* Alongside it: `iv-mon-spans` (MON 4).

*A separate piece of work is writing the web server series; these entries are catalogue only and may be superseded by it.*

#### NET 1. Sans-IO protocol state machine

`iv-sans-io` · 90 min · medium · P2 · Tracks: B4

- **New idea:** a protocol as a pure state machine.
- **Core:** A request-response protocol as bytes in, events out.
- **Follow-up 1:** Timeouts through an injected now. *Test:* simulation.
- **Follow-up 2:** Pipelining and cancellation. *Test:* scripts.
- **Follow-up 3:** Drive it from a blocking and a tokio adapter. *Test:* in-memory duplex.
- **Trade-offs:** Testability. **Concepts:** sans-IO, state machine.
- **After this:** you can test protocols with no sockets. **In projects:** protocol libraries.
- **Required first:** `iv-length-codec` (BIN 2). **Helpful:** `iv-vending-machine` (PAT 1). **Used again in:** `iv-tokio-tcp-server` (NET 13).
- **Cookbook:** similar\* `sans-io-protocol`. **Status:** to be written. **Runner:** gradable today.

#### NET 2. TCP stream reassembly

`iv-tcp-reassembly` · 60 min · hard · P2 · Tracks: B4

- **New idea:** order bytes that arrive out of order, with wrap-around.
- **Core:** Accept segments with offsets and emit the stream.
- **Follow-up 1:** Sequence wraparound. *Test:* streams crossing u32::MAX.
- **Follow-up 2:** Window limits and overlaps. *Test:* shuffled segments.
- **Follow-up 3:** FIN handling. *Test:* state tests.
- **Trade-offs:** Interval bookkeeping. **Concepts:** interval map, wrapping arithmetic.
- **After this:** you can explain TCP receive. **In projects:** network debugging.
- **Required first:** `iv-interval-booking` (COLL 10). **Helpful:** `iv-alg-iv-set` (IVL 3).
- **Cookbook:** related `udp-sockets-reliable`. **Status:** exists: `b4-tcp-reassembler`. **Runner:** gradable today.

#### NET 3. Reliable delivery over a lossy channel

`iv-reliable-udp` · 90 min · hard · P3 · Tracks: B4

- **New idea:** windows and timers recover loss.
- **Core:** Stop-and-wait with retransmit on a simulated lossy link.
- **Follow-up 1:** Go-Back-N. *Test:* all data arrives under seeded loss.
- **Follow-up 2:** Selective repeat. *Test:* retransmission bound.
- **Follow-up 3:** Adaptive timeout. *Test:* simulated delays.
- **Trade-offs:** Window sizing. **Concepts:** protocols, simulation.
- **After this:** you can design reliable transports. **In projects:** games and telemetry.
- **Required first:** `iv-dist-delivery` (DIST 2). **Helpful:** `iv-job-scheduler` (CONC 15).
- **Cookbook:** similar\* `udp-sockets-reliable`. **Status:** to be written. **Runner:** ok (simulated channel).

#### NET 4. WebSocket framing

`iv-websocket-frames` · 60 min · medium · P3 · Tracks: B4

- **New idea:** masking, fragments and control frames.
- **Core:** Encode and decode frames with masking.
- **Follow-up 1:** Fragmentation with interleaved control frames. *Test:* scripts.
- **Follow-up 2:** Protocol violations. *Test:* table.
- **Follow-up 3:** Handshake accept key (SHA-1 by hand). *Test:* RFC vector.
- **Trade-offs:** Masking purpose. **Concepts:** framing.
- **After this:** you can implement an upgrade protocol. **In projects:** realtime apps.
- **Required first:** `iv-length-codec` (BIN 2).
- **Cookbook:** similar\* `websocket-frames`. **Status:** to be written. **Runner:** gradable today.

#### NET 5. Framed TCP server with std threads

`iv-tcp-echo-framed` · 90 min · medium · P1 · Tracks: S10

- **New idea:** real sockets with timeouts and shutdown.
- **Core:** Length-framed echo with a thread per connection.
- **Follow-up 1:** Connection limit and read timeouts. *Test:* clients in tests.
- **Follow-up 2:** Graceful shutdown. *Test:* no hang.
- **Follow-up 3:** tokio version, same tests. *Test:* both pass.
- **Trade-offs:** Thread vs task per connection. **Concepts:** TcpListener, shutdown.
- **After this:** you can write a small server. **In projects:** internal services.
- **Required first:** `iv-length-codec` (BIN 2). **Helpful:** `iv-thread-pool` (CONC 9). **Used again in:** `iv-event-loop` (NET 8), `iv-tcp-lingering-close` (NET 12), `iv-tokio-tcp-server` (NET 13).
- **Cookbook:** similar\* `tcp-framed-server`. **Status:** to be written. **Runner:** loopback inside the --network none container should work but is not verified; keep tests to 127.0.0.1.

#### NET 6. Incremental HTTP/1.1 request parser

`iv-http-request` · 120 min · hard · P1 · Tracks: B4

- **New idea:** framing rules that stop request smuggling.
- **Core:** Request line and headers with need-more-bytes.
- **Follow-up 1:** Content-Length and chunked bodies. *Test:* every split point.
- **Follow-up 2:** Smuggling defences and limits. *Test:* hostile corpus.
- **Follow-up 3:** Pipelining. *Test:* concatenated requests.
- **Trade-offs:** Lenient vs strict. **Concepts:** incremental parsing, limits.
- **After this:** you can parse HTTP safely. **In projects:** proxies.
- **Required first:** `iv-length-codec` (BIN 2). **Helpful:** `iv-url-parser` (PARSE 6). **Used again in:** `iv-http-server` (NET 7), `iv-http-response-framing` (NET 11).
- **Cookbook:** similar\* `http1-parser` (framing, smuggling defence, pipelining). **Status:** exists: `b4-http1-request-parser`. **Runner:** gradable today.

#### NET 7. HTTP server on a thread pool

`iv-http-server` · 120 min · hard · P2 · Tracks: B4

- **New idea:** accept, queue, shed and drain.
- **Core:** GET with a bounded accept queue.
- **Follow-up 1:** Keep-alive and limits. *Test:* raw socket tests.
- **Follow-up 2:** Load shedding with 503. *Test:* saturation test.
- **Follow-up 3:** Graceful drain. *Test:* in-flight finish.
- **Trade-offs:** Backpressure at accept. **Concepts:** sockets, pool, parsing.
- **After this:** you can run a server under overload. **In projects:** web services.
- **Required first:** `iv-http-request` (NET 6). **Helpful:** `iv-thread-pool` (CONC 9), `iv-graceful-shutdown` (ASYNC 7).
- **Cookbook:** similar\* `mini-http-server-thread-pool`. **Status:** to be written. **Runner:** loopback not verified in the sandbox.

#### NET 8. Readiness event loop

`iv-event-loop` · 120 min · hard · P3 · Tracks: Y

- **New idea:** one thread drives many sockets by readiness.
- **Core:** Echo many clients on one thread with readiness events.
- **Follow-up 1:** Partial writes and write interest. *Test:* slow reader.
- **Follow-up 2:** Timers. *Test:* bounds.
- **Follow-up 3:** Edge vs level triggered. *Test:* both.
- **Trade-offs:** Why edge triggering needs draining. **Concepts:** reactor.
- **After this:** you can explain async reactors. **In projects:** high-connection servers.
- **Required first:** `iv-tcp-echo-framed` (NET 5). **Helpful:** `iv-executor` (ASYNC 2). **Used again in:** `iv-mio-reactor-api` (NET 10).
- **Cookbook:** similar\* `mio-event-loop`, similar `kqueue-epoll-raw`. **Status:** to be written. **Runner:** needs mio or libc, neither in docker/deps.

#### NET 9. A pipelined Tokio TCP connection task

`iv-tokio-tcp-conn` · 120 min · hard · P2 · prep · Tracks: NET

- **New idea:** one task multiplexes socket input, replies and shutdown with bounded buffers.
- **Core:** Write a line-protocol server and client on loopback: split the stream, read_buf into a buffer, handle every complete request in one batch with one write, and select over input, a reply channel (reading only under a cap) and a stop signal.
- **Follow-up 1:** pushed messages and a Notify kill that interrupts a stalled write. *Test:* tiny queue and a non-reading peer, the connection closes ; fragmented and pipelined input and a protocol error :: byte-at-a-time and many-in-one writes give ordered replies, bad input gets an error then EOF.
- **Required first:** `iv-length-codec` (BIN 2), `iv-cancel-safety` (ASYNC 6).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

#### NET 10. Drive nonblocking sockets with mio

`iv-mio-reactor-api` · 90 min · hard · P2 · prep · Tracks: NET

- **New idea:** the operating system reports edges per token and the program must try operations until WouldBlock.
- **Core:** Register a listener and streams with a mio Poll, run an echo loop, and stop it with a mio Waker (needs mio vendored in the runner and loopback).
- **Follow-up 1:** nonblocking connect completion. *Test:* refused and accepted cases ; deregister on drop :: token map empty afterwards.
- **Required first:** `iv-event-loop` (NET 8).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** needs mio, which is not in docker/deps.

#### NET 11. Where an HTTP response ends

`iv-http-response-framing` · 90 min · hard · P2 · prep · Tracks: NET

- **New idea:** method, status and headers together decide how the message body is delimited.
- **Core:** Write a response serialiser and a matching incremental reader covering length, chunked, HEAD, 1xx, 204, 304, Connection close and 100 Continue.
- **Follow-up 1:** streaming body flushed per chunk. *Test:* writer that records each write and flush ; HTTP/1.0 peer :: no chunked framing, close delimits.
- **Required first:** `iv-http-request` (NET 6).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

#### NET 12. Close a socket without destroying your reply

`iv-tcp-lingering-close` · 60 min · medium · P2 · prep · Tracks: NET

- **New idea:** closing with unread input resets the connection, so shut down writing and drain within bounds first.
- **Core:** Send an error reply to a client that is still uploading, then close with shutdown of the write side and a drain bounded in bytes and time (needs loopback).
- **Follow-up 1:** byte cap on the drain. *Test:* peer that keeps sending is dropped after the cap ; clean peer :: reads whole reply then EOF.
- **Required first:** `iv-tcp-echo-framed` (NET 5).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

#### NET 13. A task per connection with tokio sockets

`iv-tokio-tcp-server` · 60 min · medium · P2 · prep · Tracks: NET

- **New idea:** async sockets put the same sans-IO parser behind a task per connection with a timeout on each read.
- **Core:** Write a tokio line or frame server that reuses a pure parser, handles pipelining and times out idle reads (needs loopback).
- **Follow-up 1:** paused-clock idle timeout over an in-memory duplex stream. *Test:* advance virtual time ; shutdown of all connection tasks :: all handles finish.
- **Required first:** `iv-tcp-echo-framed` (NET 5), `iv-sans-io` (NET 1).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

### WEB: Web services

HTTP concerns without hiding them behind a framework. *Core series, 9 parts, about 10 h. Ends in `cap-jobsvc`.* Alongside it: `iv-mon-metrics` (MON 2), `iv-mon-spans` (MON 4).

*A separate piece of work is writing the web server series; these entries are catalogue only and may be superseded by it.*

#### WEB 1. URL router with parameters

`iv-router` · 60 min · medium · P1 · Tracks: B1

- **New idea:** match paths by segments with precedence.
- **Core:** add(method, pattern, handler) and match.
- **Follow-up 1:** Parameters and wildcards with precedence static > param > wildcard. *Test:* brute-force matcher.
- **Follow-up 2:** 405 vs 404 and conflicts at registration. *Test:* tables.
- **Follow-up 3:** Middleware per prefix. *Test:* order log.
- **Trade-offs:** Trie of segments vs regex list. **Concepts:** segment trie, precedence.
- **After this:** you can design routing. **In projects:** web frameworks and gateways.
- **Required first:** `iv-trie` (COLL 6). **Helpful:** `iv-glob` (PARSE 10). **Used again in:** `iv-axum-api` (WEB 3).
- **Cookbook:** similar `mini-http-server-thread-pool` (router with parameters), related `axum-router-state`. **Status:** to be written. **Runner:** gradable today.

#### WEB 2. Cursor pagination

`iv-cursor-pagination` · 45 min · medium · P2 · Tracks: B1

- **New idea:** a cursor encodes the last key seen.
- **Core:** Page through an ordered set with opaque cursors.
- **Follow-up 1:** Stable under concurrent inserts and deletes. *Test:* no duplicate, no skip of stable rows.
- **Follow-up 2:** Composite sort keys. *Test:* oracle.
- **Follow-up 3:** Tamper-evident cursors. *Test:* modified cursor rejected.
- **Trade-offs:** Offset vs keyset. **Concepts:** keyset pagination, encoding.
- **After this:** you can paginate changing data. **In projects:** APIs.
- **Required first:** nothing. **Helpful:** `iv-base64` (TEXT 3).
- **Cookbook:** similar `rest-api-patterns`. **Status:** to be written. **Runner:** gradable today.

#### WEB 3. A small JSON API with axum

`iv-axum-api` · 120 min · medium · P1 · Tracks: B1

- **New idea:** handlers, extractors and one error shape.
- **Core:** CRUD routes with shared state.
- **Follow-up 1:** One error shape including extractor rejections. *Test:* oneshot tests.
- **Follow-up 2:** Validation collecting all field errors. *Test:* tables.
- **Follow-up 3:** Idempotency key on POST. *Test:* replay.
- **Trade-offs:** Extractor design. **Concepts:** axum, extractors, errors.
- **After this:** you can build a JSON API. **In projects:** any backend.
- **Required first:** `iv-error-layers` (ERR 2). **Helpful:** `iv-router` (WEB 1). **Used again in:** `iv-sse-replay` (WEB 7), `iv-axum-request-tracing` (WEB 8).
- **Cookbook:** similar `axum-router-state`, similar `axum-errors`, similar `backend-request-validation-extractors`. **Status:** staged draft: `b1-api-error-response`. **Runner:** ok (axum, tower oneshot; no sockets).

#### WEB 4. Tower-style Service and Layer

`iv-middleware-tower` · 90 min · hard · P2 · Tracks: B2

- **New idea:** readiness before calling applies backpressure.
- **Core:** Define Service and a logging layer.
- **Follow-up 1:** Timeout and concurrency limit with poll_ready. *Test:* backpressure tests.
- **Follow-up 2:** Retry layer. *Test:* attempt counts.
- **Follow-up 3:** Stack builder. *Test:* order.
- **Trade-offs:** Why poll_ready. **Concepts:** traits, futures, backpressure.
- **After this:** you can write middleware. **In projects:** tower and axum services.
- **Required first:** `iv-fn-pipeline` (ITER 2). **Helpful:** `iv-async-trait-object` (ASYNC 11).
- **Cookbook:** similar `tower-service-layer` (its trait has no poll_ready). **Status:** staged draft: `b2-fix-poll-ready`. **Runner:** ok (tower vendored).

#### WEB 5. HTTP cache

`iv-http-cache` · 60 min · medium · P3 · Tracks: B1

- **New idea:** validators and freshness.
- **Core:** Cache by method and URL honouring max-age.
- **Follow-up 1:** ETag revalidation. *Test:* origin call counts.
- **Follow-up 2:** Vary. *Test:* key tests.
- **Follow-up 3:** Size limit eviction. *Test:* model.
- **Trade-offs:** Freshness rules. **Concepts:** cache policy, headers.
- **After this:** you can cache responses correctly. **In projects:** clients and proxies.
- **Required first:** `iv-lru` (CACHE 1). **Helpful:** `iv-ttl-kv` (CACHE 2). **Used again in:** `iv-http-range` (WEB 9).
- **Cookbook:** similar `http-caching-conditional`. **Status:** to be written. **Runner:** gradable today.

#### WEB 6. Webhook delivery

`iv-webhook-sender` · 60 min · medium · P3 · Tracks: B5

- **New idea:** signed, retried, ordered delivery.
- **Core:** Signed delivery with retries over a fake transport.
- **Follow-up 1:** Per-endpoint backoff and dead letters. *Test:* fake clock.
- **Follow-up 2:** Ordering per key. *Test:* property.
- **Follow-up 3:** Replay protection on the receiver. *Test:* window tests.
- **Trade-offs:** At-least-once. **Concepts:** signatures, retries.
- **After this:** you can integrate with third parties. **In projects:** payment and event hooks.
- **Required first:** `iv-retry-backoff` (RESIL 1). **Helpful:** `iv-jwt-hmac` (SEC 4).
- **Cookbook:** similar\* `webhooks-signatures`. **Status:** to be written. **Runner:** gradable today.

#### WEB 7. Server-sent events with resume

`iv-sse-replay` · 60 min · medium · P2 · prep · Tracks: WEB

- **New idea:** numbered events let a reconnecting client resume where it stopped.
- **Core:** Serve an append-only event list as SSE and replay only ids after Last-Event-ID.
- **Follow-up 1:** live tail after replay without gaps or duplicates. *Test:* publish at controlled points and collect a finite stream ; bounded retention :: a too-old id gets an explicit reset response.
- **Required first:** `iv-axum-api` (WEB 3).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

#### WEB 8. Request ids and spans in axum middleware

`iv-axum-request-tracing` · 60 min · medium · P2 · prep · Tracks: WEB

- **New idea:** middleware wraps every request, so cross-cutting context belongs there.
- **Core:** Write a from_fn layer that validates or generates x-request-id, sets it on every response and instruments the handler with a span.
- **Follow-up 1:** capture tracing output with a test subscriber. *Test:* log lines inside the handler carry the request id ; layer order :: errors from an inner layer still get the header.
- **Required first:** `iv-axum-api` (WEB 3).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

#### WEB 9. Serve byte ranges

`iv-http-range` · 45 min · medium · P2 · prep · Tracks: WEB

- **New idea:** a range header is ignored, satisfied or refused depending on syntax and resource length.
- **Core:** Parse a single bytes range against a length and build the 200, 206 or 416 response.
- **Follow-up 1:** suffix and open-ended forms. *Test:* table including length zero ; If-Range with an ETag :: stale tag returns the full body.
- **Required first:** `iv-http-cache` (WEB 5).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

### SEC: Security

Write the safe version and the attack that breaks the unsafe one. *Core series, 8 parts, about 8 h. Ends in `cap-backend`.* Alongside it: `iv-fuzz-harness` (TEST 5).

#### SEC 1. Join an untrusted path safely

`iv-sec-safe-path` · 45 min · medium · P1 · Tracks: S9

- **New idea:** resolve before you check.
- **Core:** Resolve a user path under a root without escaping it.
- **Follow-up 1:** Percent-encoded and mixed separators. *Test:* attack corpus.
- **Follow-up 2:** Symlinks through a fake filesystem trait. *Test:* planted links.
- **Follow-up 3:** Check-then-use race shown and closed. *Test:* scripted fake.
- **Trade-offs:** Normalising vs resolving. **Concepts:** path traversal, TOCTOU.
- **After this:** you can serve files safely. **In projects:** uploads and static files.
- **Required first:** nothing. **Helpful:** `iv-url-parser` (PARSE 6). **Used again in:** `iv-sec-egress` (SEC 7), `iv-sbx-vfs-jail` (SBX 4).
- **Cookbook:** similar\* `security-pitfalls-secure-coding` (path traversal and TOCTOU pairs), related `std-path-pathbuf-ffi-strings`. **Status:** to be written. **Runner:** gradable today.

#### SEC 2. Escape for the right context

`iv-sec-injection` · 60 min · medium · P2 · Tracks: S2

- **New idea:** escaping depends on where text goes.
- **Core:** HTML, attribute and URL escaping with types that prevent double escaping.
- **Follow-up 1:** A parameterised query builder. *Test:* hostile strings never change structure.
- **Follow-up 2:** Log injection and header splitting. *Test:* corpus.
- **Follow-up 3:** Types that make unescaped output a compile error. *Test:* positive tests.
- **Trade-offs:** Context-sensitive escaping. **Concepts:** newtypes, injection.
- **After this:** you can stop injection with types. **In projects:** web apps.
- **Required first:** `iv-units-newtypes` (TRAIT 2). **Used again in:** `iv-sbx-command-policy` (SBX 2).
- **Cookbook:** similar\* `security-pitfalls-secure-coding`. **Status:** to be written. **Runner:** gradable today.

#### SEC 3. Compare secrets in constant time

`iv-sec-ct-compare` · 45 min · medium · P2 · Tracks: F5

- **New idea:** time can leak secrets.
- **Core:** ct_eq and ct_select over bytes.
- **Follow-up 1:** A simulated timing attack on a counted comparison. *Test:* attack succeeds on ==, fails on yours.
- **Follow-up 2:** Length handling. *Test:* tables.
- **Follow-up 3:** Zeroise on drop. *Test:* observed through a test hook.
- **Trade-offs:** What the optimiser may do. **Concepts:** side channels.
- **After this:** you can compare secrets safely. **In projects:** authentication.
- **Required first:** nothing. **Used again in:** `iv-jwt-hmac` (SEC 4), `iv-sec-passwords` (SEC 8).
- **Cookbook:** similar\* `constant-time-side-channels`. **Status:** to be written. **Runner:** ok; timing itself is not asserted, an operation-counting model is.

#### SEC 4. HS256 token verification

`iv-jwt-hmac` · 60 min · medium · P2 · Tracks: B5

- **New idea:** verify a signed token, never trust its header.
- **Core:** Sign and verify with constant-time comparison.
- **Follow-up 1:** exp, nbf, audience with leeway. *Test:* fake clock.
- **Follow-up 2:** Algorithm confusion and none. *Test:* attack corpus.
- **Follow-up 3:** Key rotation by key id. *Test:* model.
- **Trade-offs:** Why not roll your own in production. **Concepts:** HMAC, constant time, parsing.
- **After this:** you can authenticate requests. **In projects:** APIs.
- **Required first:** `iv-sec-ct-compare` (SEC 3). **Helpful:** `iv-base64` (TEXT 3). **Used again in:** `iv-webhook-sender` (WEB 6), `iv-sessions` (SEC 5).
- **Cookbook:** similar\* `auth-basic-bearer-jwt`. **Status:** staged draft: `b5-fix-webhook-verify`. **Runner:** ok (sha2, base64).

#### SEC 5. Session store

`iv-sessions` · 60 min · medium · P2 · Tracks: B5

- **New idea:** server state with expiry and revocation.
- **Core:** Create, get, expire sessions with a fake clock.
- **Follow-up 1:** Sliding and absolute expiry. *Test:* fake-clock tables.
- **Follow-up 2:** Rotate on login and revoke all for a user. *Test:* model.
- **Follow-up 3:** Signed cookie values. *Test:* tamper rejected (sha2 vendored).
- **Trade-offs:** Server sessions vs tokens. **Concepts:** TTL, HMAC.
- **After this:** you can manage logins. **In projects:** web apps.
- **Required first:** `iv-ttl-kv` (CACHE 2). **Helpful:** `iv-jwt-hmac` (SEC 4).
- **Cookbook:** similar `cookies-sessions-csrf`, similar `hmac-hkdf-pbkdf2`. **Status:** to be written. **Runner:** gradable today.

#### SEC 6. Authorisation policy engine

`iv-authz` · 45 min · medium · P3 · Tracks: B1

- **New idea:** default deny as a pure function.
- **Core:** Roles to permissions with default deny.
- **Follow-up 1:** Deny overrides and ownership rules. *Test:* truth-table oracle.
- **Follow-up 2:** Tenant isolation. *Test:* property over random subjects.
- **Follow-up 3:** Audit trail of decisions. *Test:* log.
- **Trade-offs:** RBAC vs ABAC. **Concepts:** policy as pure functions.
- **After this:** you can test permissions exhaustively. **In projects:** multi-tenant apps.
- **Required first:** nothing.
- **Cookbook:** similar\* `backend-authz-rbac-abac-policies`. **Status:** to be written. **Runner:** gradable today.

#### SEC 7. An outbound request allowlist

`iv-sec-egress` · 60 min · medium · P3 · Tracks: B5

- **New idea:** check the address you will really connect to.
- **Core:** Decide whether a URL may be fetched.
- **Follow-up 1:** Private and link-local ranges, IPv4 and IPv6 forms. *Test:* corpus.
- **Follow-up 2:** Redirects and name resolution through a fake resolver. *Test:* rebinding script.
- **Follow-up 3:** Per-tenant rules. *Test:* truth table.
- **Trade-offs:** Check after resolution. **Concepts:** SSRF, CIDR.
- **After this:** you can block request forgery. **In projects:** webhooks and fetchers.
- **Required first:** `iv-alg-bit-fields` (BIT 2). **Helpful:** `iv-sec-safe-path` (SEC 1).
- **Cookbook:** similar `security-pitfalls-secure-coding`, similar `std-net-tcp-udp-ip-addr`. **Status:** to be written. **Runner:** gradable today.

#### SEC 8. Password accounts and lockout

`iv-sec-passwords` · 90 min · medium · P3 · Tracks: B5

- **New idea:** slow salted hashing and single-use tokens.
- **Core:** Salted, iterated hashing with parameters stored in the string.
- **Follow-up 1:** Verify and rehash on parameter change. *Test:* tables.
- **Follow-up 2:** Lockout and one-time reset tokens on a fake clock. *Test:* scripts.
- **Follow-up 3:** TOTP with a window and replay protection. *Test:* RFC vectors.
- **Trade-offs:** Cost factors. **Concepts:** KDF, tokens.
- **After this:** you can run account flows. **In projects:** any product with logins.
- **Required first:** `iv-sec-ct-compare` (SEC 3). **Helpful:** `iv-ttl-kv` (CACHE 2).
- **Cookbook:** similar\* `backend-password-accounts-totp-mfa`. **Status:** to be written. **Runner:** ok (sha2); PBKDF2 by hand, argon2 not vendored.

### SBX: Task sandboxing

Limit what untrusted work can do; know what only the kernel can enforce. *Core series, 6 parts, about 7 h. Ends in `cap-jobsched`.* Alongside it: `iv-mon-flight-recorder` (MON 3).

#### SBX 1. Capability-based host functions

`iv-sbx-capabilities` · 60 min · medium · P2 · Tracks: L5

- **New idea:** code can use only what it is handed.
- **Core:** Guest code can only call what its capability object grants.
- **Follow-up 1:** Attenuate: read-only and path-scoped capabilities. *Test:* escape corpus.
- **Follow-up 2:** Revocation. *Test:* later calls fail.
- **Follow-up 3:** Make forging impossible through the type system. *Test:* positive tests, private constructors.
- **Trade-offs:** Ambient authority. **Concepts:** capabilities, visibility, traits.
- **After this:** you can design plugin APIs. **In projects:** plugin hosts.
- **Required first:** `iv-typestate-conn` (TRAIT 7).
- **Cookbook:** related `typestate-protocol`. **Status:** to be written. **Runner:** gradable today.

#### SBX 2. Decide what a task may execute

`iv-sbx-command-policy` · 45 min · medium · P3 · Tracks: S10

- **New idea:** validate argv, never build a shell string.
- **Core:** Validate argv against a policy with no shell.
- **Follow-up 1:** Environment allowlist and secret scrubbing. *Test:* corpus.
- **Follow-up 2:** Argument injection through leading dashes. *Test:* corpus.
- **Follow-up 3:** Audit records. *Test:* golden.
- **Trade-offs:** Allow lists vs deny lists. **Concepts:** policy, injection.
- **After this:** you can gate job commands. **In projects:** CI and workflow runners.
- **Required first:** `iv-shell-line` (PARSE 8). **Helpful:** `iv-sec-injection` (SEC 2).
- **Cookbook:** similar `security-pitfalls-secure-coding`. **Status:** to be written. **Runner:** ok; nothing is executed.

#### SBX 3. Hierarchical resource quotas

`iv-sbx-quota` · 60 min · medium · P2 · Tracks: C1

- **New idea:** charge resources up a budget tree.
- **Core:** Charge memory and task counts against a tree of budgets.
- **Follow-up 1:** RAII release. *Test:* no leak after panics.
- **Follow-up 2:** Atomic charge up the chain, all or nothing. *Test:* concurrent stress never exceeds a limit.
- **Follow-up 3:** Fair share when siblings compete. *Test:* model.
- **Trade-offs:** Accounting vs enforcement. **Concepts:** RAII, hierarchical accounting.
- **After this:** you can enforce tenant limits. **In projects:** multi-tenant platforms.
- **Required first:** `iv-raii-pool` (SMART 1). **Helpful:** `iv-semaphore` (CONC 5).
- **Cookbook:** related `std-alloc-layout-global-allocator`. **Status:** to be written. **Runner:** gradable today.

#### SBX 4. A virtual filesystem jail

`iv-sbx-vfs-jail` · 90 min · medium · P2 · Tracks: S9

- **New idea:** a private filesystem view per task.
- **Core:** Give a task an in-memory filesystem rooted at its own directory with size limits.
- **Follow-up 1:** Read-only mounts and overlay writes. *Test:* model.
- **Follow-up 2:** Symlink and .. escapes. *Test:* corpus.
- **Follow-up 3:** Diff of what the task changed. *Test:* oracle.
- **Trade-offs:** Overlay semantics. **Concepts:** path resolution, overlay.
- **After this:** you can isolate task files. **In projects:** build sandboxes.
- **Required first:** `iv-sec-safe-path` (SEC 1). **Helpful:** `iv-file-system-tree` (PAT 11).
- **Cookbook:** related `toy-filesystem`, related `security-pitfalls-secure-coding`. **Status:** to be written. **Runner:** gradable today.

#### SBX 5. Run untrusted bytecode with limits

`iv-sbx-fuel` · 90 min · medium · P1 · Tracks: Y6

- **New idea:** meter every instruction.
- **Core:** An interpreter that stops on instruction fuel, stack depth and memory limits with typed traps.
- **Follow-up 1:** Every program terminates. *Test:* random bytecode never panics or hangs.
- **Follow-up 2:** Accounting is exact. *Test:* fuel used equals a reference count.
- **Follow-up 3:** Resumable after refuel. *Test:* same result as one run.
- **Trade-offs:** What a limit protects against. **Concepts:** metering, traps.
- **After this:** you can stop runaway scripts. **In projects:** smart contracts and plugins.
- **Required first:** `iv-bytecode-vm` (LANG 5).
- **Cookbook:** similar `wasm-binary-interpreter`, similar `bytecode-vm`. **Status:** to be written (partly practised by `y6-wasm-stack-interpreter`). **Runner:** gradable today.

#### SBX 6. Supervise a task's lifetime

`iv-sbx-supervisor` · 90 min · hard · P2 · Tracks: S10

- **New idea:** lifecycle with grace, kill and restart.
- **Core:** A state machine: start, run, graceful stop, kill after a grace period, on a fake clock.
- **Follow-up 1:** Restart policy with backoff. *Test:* scripted exits.
- **Follow-up 2:** Output captured with a byte cap. *Test:* hostile output.
- **Follow-up 3:** Orphan cleanup of child groups. *Test:* fake process tree.
- **Trade-offs:** What the kernel must enforce. **Concepts:** state machine, timeouts.
- **After this:** you can supervise workers. **In projects:** job runners.
- **Required first:** `iv-vending-machine` (PAT 1). **Helpful:** `iv-retry-backoff` (RESIL 1), `iv-process-pipeline` (IO 7).
- **Cookbook:** similar `process-pipes-signals`. **Status:** to be written. **Runner:** ok with a fake process trait; real process isolation, namespaces and seccomp cannot be graded and the cookbook has no recipe for them.

### STORE: Storage engine

Pages, logs and trees, one mechanism at a time. *Core series, 20 parts, about 28 h. Ends in `cap-kvstore`.* Alongside it: `iv-test-failpoints` (TEST 7), `iv-mon-metrics` (MON 2).

#### STORE 1. Tuple serialisation with schema

`iv-row-serialization` · 60 min · medium · P2 · Tracks: F7

- **New idea:** a row layout with nulls and ordered keys.
- **Core:** Encode rows of ints, strings and nulls with a null bitmap.
- **Follow-up 1:** Read one column without decoding the row. *Test:* offsets.
- **Follow-up 2:** Order-preserving key encoding. *Test:* byte order equals value order.
- **Follow-up 3:** Schema evolution adding columns. *Test:* old rows readable.
- **Trade-offs:** Fixed vs variable columns. **Concepts:** layout, byte order, versioning.
- **After this:** you can design record formats. **In projects:** databases.
- **Required first:** `iv-varint-codec` (BIN 1). **Used again in:** `iv-slotted-page` (STORE 3).
- **Cookbook:** similar `byte-order-keys`, similar `std-hash-collections-io-serialization-formats`. **Status:** to be written (partly practised by `f2-validity-bitmap-column`). **Runner:** gradable today.

#### STORE 2. Write-ahead log with torn-write recovery

`iv-wal` · 60 min · medium · P1 · Tracks: S9

- **New idea:** append checksummed records and trust only the good prefix.
- **Core:** Append checksummed records and replay to the first bad one.
- **Follow-up 1:** Truncate the torn tail. *Test:* every cut point.
- **Follow-up 2:** Batches atomic as a whole. *Test:* partial batch invisible.
- **Follow-up 3:** Group commit with a sync counter. *Test:* fewer syncs than records under threads.
- **Trade-offs:** fsync semantics. **Concepts:** framing, CRC, recovery.
- **After this:** you can make state durable. **In projects:** databases and queues.
- **Required first:** `iv-varint-codec` (BIN 1). **Used again in:** `iv-memtable-flush` (STORE 5), `iv-checkpoint-recovery` (STORE 14), `iv-effect-log` (STORE 17), `iv-group-commit` (STORE 18), `iv-crash-ordered-files` (STORE 20), `iv-partitioned-log` (DIST 12), `iv-two-phase-commit` (DIST 15), `iv-raft-replication` (DIST 19), `iv-lin-event-store` (LIN 4), `iv-test-failpoints` (TEST 7).
- **Cookbook:** exact `wal-append-log` (append, scan, tail classification), similar `logging-schemes-group-commit`. **Status:** exists: `y9-kv-store-wal`. **Runner:** gradable today.

#### STORE 3. Slotted page

`iv-slotted-page` · 60 min · medium · P1 · Tracks: F2

- **New idea:** a slot directory lets records move inside a page.
- **Core:** insert, get, delete variable-length records in a fixed 4 KiB array.
- **Follow-up 1:** Compaction keeping slot ids stable. *Test:* random ops against a map model.
- **Follow-up 2:** Update that grows a record. *Test:* space accounting.
- **Follow-up 3:** Forwarding to another page. *Test:* lookups through one hop.
- **Trade-offs:** Slot directory design. **Concepts:** page layout, indirection.
- **After this:** you can lay out pages. **In projects:** storage engines.
- **Required first:** `iv-row-serialization` (STORE 1). **Used again in:** `iv-fs-blocks` (OS 5), `iv-heap-file` (STORE 4).
- **Cookbook:** similar\* `slotted-pages-heap-file` (page, heap file, forwarding). **Status:** exists: `f2-slotted-page`. **Runner:** gradable today.

#### STORE 4. Heap file over pages

`iv-heap-file` · 90 min · medium · P2 · Tracks: F2

- **New idea:** records addressed across many pages.
- **Core:** Record ids, insert, get, delete, scan.
- **Follow-up 1:** Free-space map. *Test:* pages touched counter.
- **Follow-up 2:** Overflow pages for large records. *Test:* round trip.
- **Follow-up 3:** Reopen from the page store. *Test:* model equality.
- **Trade-offs:** RID stability. **Concepts:** pages, free space, RIDs.
- **After this:** you can store tables. **In projects:** databases.
- **Required first:** `iv-slotted-page` (STORE 3). **Helpful:** `iv-slab` (MEM 1). **Used again in:** `iv-buffer-pool` (STORE 9).
- **Cookbook:** similar\* `slotted-pages-heap-file`. **Status:** to be written. **Runner:** ok (an in-memory page store stands in for disk).

#### STORE 5. Memtable and flush

`iv-memtable-flush` · 60 min · medium · P1 · Tracks: S4

- **New idea:** writes go to memory, deletes are markers.
- **Core:** Ordered memtable with tombstones and a size threshold.
- **Follow-up 1:** Read path through memtable and frozen tables. *Test:* model.
- **Follow-up 2:** Merged range scan. *Test:* oracle.
- **Follow-up 3:** Writes during flush. *Test:* no loss.
- **Trade-offs:** Immutable memtables. **Concepts:** LSM, tombstones.
- **After this:** you can buffer writes in order. **In projects:** LSM stores.
- **Required first:** `iv-wal` (STORE 2). **Helpful:** `iv-ordered-map-skiplist` (COLL 14). **Used again in:** `iv-compaction` (STORE 6), `iv-bloom-sstable` (STORE 7).
- **Cookbook:** similar `mini-kv-store-code`. **Status:** to be written. **Runner:** gradable today.

#### STORE 6. Merge compaction

`iv-compaction` · 60 min · medium · P1 · Tracks: S5

- **New idea:** merge sorted runs, keeping the newest.
- **Core:** Merge k sorted runs keeping the newest version of each key.
- **Follow-up 1:** Drop tombstones only when safe. *Test:* model with an older level.
- **Follow-up 2:** Lazy iterator form. *Test:* laziness proven.
- **Follow-up 3:** Size-tiered pick policy and write amplification count. *Test:* scripted workloads.
- **Trade-offs:** Leveled vs tiered. **Concepts:** k-way merge, versions.
- **After this:** you can reclaim space. **In projects:** LSM stores.
- **Required first:** `iv-memtable-flush` (STORE 5). **Helpful:** `iv-peekmore-chunks` (ITER 4). **Used again in:** `iv-external-sort` (QUERY 3).
- <details><summary>Links hidden until asked</summary>helpful: `iv-binary-heap` (COLL 4)</details>
- **Cookbook:** similar\* `kway-merge-family` (its LSM compaction part), similar `lsm-compaction-strategies`. **Status:** staged draft: `s6-merge-sorted-streams`. **Runner:** gradable today.

#### STORE 7. SSTable with a sparse index and Bloom filter

`iv-bloom-sstable` · 120 min · medium · P1 · Tracks: F4

- **New idea:** an immutable sorted file with index and filter.
- **Core:** Build from sorted pairs; point get reads one block.
- **Follow-up 1:** Sparse index and range iterator. *Test:* oracle, block-read counter.
- **Follow-up 2:** Bloom filter to skip absent keys. *Test:* false-positive bound.
- **Follow-up 3:** Corruption detection by checksum. *Test:* flipped bytes.
- **Trade-offs:** Block size. **Concepts:** file format, index, filter.
- **After this:** you can read from disk with few seeks. **In projects:** LSM stores.
- **Required first:** `iv-memtable-flush` (STORE 5). **Helpful:** `iv-bloom` (CACHE 4).
- **Cookbook:** similar `mini-kv-store-code` (SSTable with sparse index and bloom). **Status:** to be written. **Runner:** ok (bytes in a Vec stand for the file).

#### STORE 8. LRU-K replacer

`iv-lru-k` · 60 min · medium · P2 · Tracks: F4

- **New idea:** history of accesses picks the victim.
- **Core:** record_access, set_evictable, evict.
- **Follow-up 1:** Infinite backward distance rules. *Test:* oracle that scans history.
- **Follow-up 2:** remove and size. *Test:* edge tests.
- **Follow-up 3:** Scan resistance demonstration. *Test:* hit-count comparison.
- **Trade-offs:** History cost. **Concepts:** replacement policy, ordered sets.
- **After this:** you can resist scans. **In projects:** buffer pools.
- **Required first:** `iv-lru` (CACHE 1).
- <details><summary>Links hidden until asked</summary>helpful: `iv-indexed-pq` (COLL 8)</details>
- **Cookbook:** similar\* `lru-k-replacer-disk-scheduler`. **Status:** to be written. **Runner:** gradable today.

#### STORE 9. Buffer pool with eviction

`iv-buffer-pool` · 120 min · hard · P1 · Tracks: F4

- **New idea:** pin counts protect pages in use.
- **Core:** fetch and unpin pages with a pin count and LRU eviction over a counted disk.
- **Follow-up 1:** RAII page guard and dirty write-back. *Test:* disk contents equal the model.
- **Follow-up 2:** CLOCK or LRU-K policy behind a trait. *Test:* scripted traces.
- **Follow-up 3:** Concurrent access. *Test:* no pinned page evicted under stress.
- **Trade-offs:** Latches vs pins. **Concepts:** pinning, replacement, guards.
- **After this:** you can cache disk pages. **In projects:** databases.
- **Required first:** `iv-heap-file` (STORE 4). **Helpful:** `iv-lru` (CACHE 1), `iv-raii-pool` (SMART 1). **Used again in:** `iv-extendible-hash` (STORE 12), `iv-disk-bptree` (STORE 13), `iv-checkpoint-recovery` (STORE 14).
- **Cookbook:** similar\* `buffer-pool-clock`, similar `lru-k-replacer-disk-scheduler`. **Status:** to be written. **Runner:** gradable today.

#### STORE 10. Column encodings with zone maps

`iv-columnar-encodings` · 90 min · medium · P2 · Tracks: F2

- **New idea:** store by column and skip with zone maps.
- **Core:** Dictionary and run-length encoding with decode.
- **Follow-up 1:** Bit packing and delta. *Test:* round trip.
- **Follow-up 2:** Filter on encoded data. *Test:* oracle, decoded-values counter.
- **Follow-up 3:** Min/max zone maps skipping blocks. *Test:* blocks-read counter.
- **Trade-offs:** Choosing an encoding. **Concepts:** encodings, pushdown.
- **After this:** you can speed analytics. **In projects:** warehouses.
- **Required first:** `iv-varint-codec` (BIN 1). **Helpful:** `iv-alg-bit-fields` (BIT 2).
- **Cookbook:** similar\* `columnar-encodings`. **Status:** to be written (partly practised by `f5-bit-packed-vec`, `f7-posting-list-codec`). **Runner:** gradable today.

#### STORE 11. Bitmap index

`iv-bitmap-index` · 60 min · medium · P3 · Tracks: F5

- **New idea:** one bitmap per value.
- **Core:** Equality bitmaps over a low-cardinality column.
- **Follow-up 1:** Range queries with range encoding. *Test:* scan oracle.
- **Follow-up 2:** Word-aligned run compression. *Test:* round trip and size.
- **Follow-up 3:** AND and OR across columns. *Test:* oracle.
- **Trade-offs:** Cardinality limits. **Concepts:** bitmaps, compression.
- **After this:** you can filter fast. **In projects:** analytics.
- **Required first:** `iv-bitset-roaring` (COLL 9).
- **Cookbook:** similar\* `hash-and-bitmap-indexes`. **Status:** to be written. **Runner:** gradable today.

#### STORE 12. Extendible hash index

`iv-extendible-hash` · 90 min · medium · P2 · Tracks: F4

- **New idea:** grow a hash index one bucket at a time.
- **Core:** Directory with bucket splits.
- **Follow-up 1:** Directory doubling invariants. *Test:* checker.
- **Follow-up 2:** Delete and merge. *Test:* model.
- **Follow-up 3:** Duplicate keys with overflow. *Test:* pathological hashes.
- **Trade-offs:** vs linear hashing. **Concepts:** hashing, directory.
- **After this:** you can index on disk. **In projects:** databases.
- **Required first:** `iv-buffer-pool` (STORE 9). **Helpful:** `iv-hashmap-chaining` (COLL 3).
- **Cookbook:** similar\* `hash-and-bitmap-indexes`. **Status:** to be written. **Runner:** gradable today.

#### STORE 13. B+ tree on pages

`iv-disk-bptree` · 120 min · hard · P1 · Tracks: F2

- **New idea:** tree nodes live in pages.
- **Core:** Serialised nodes in fixed pages with splits.
- **Follow-up 1:** Range scan over sibling links. *Test:* BTreeMap oracle.
- **Follow-up 2:** Bulk load. *Test:* height and fill bounds.
- **Follow-up 3:** Reopen from pages. *Test:* model equality.
- **Trade-offs:** Node encoding; fan-out. **Concepts:** B+ tree, serialisation.
- **After this:** you can build a persistent index. **In projects:** databases.
- **Required first:** `iv-bptree-memory` (COLL 16). **Helpful:** `iv-buffer-pool` (STORE 9).
- **Cookbook:** similar `bplus-tree` (in memory), similar `btree-bulk-load-prefix-compression`. **Status:** to be written. **Runner:** ok; cut to insert and search for the 2-hour box, delete moves to the capstone.

#### STORE 14. Redo and undo recovery

`iv-checkpoint-recovery` · 120 min · hard · P2 · Tracks: S9

- **New idea:** redo and undo bring pages back after a crash.
- **Core:** Redo-only recovery of committed transactions from a log.
- **Follow-up 1:** Steal policy with undo. *Test:* crash at every log position.
- **Follow-up 2:** Checkpoints. *Test:* replay length bound.
- **Follow-up 3:** Compensation records so recovery can crash too. *Test:* repeated crashes.
- **Trade-offs:** Steal, force. **Concepts:** ARIES ideas.
- **After this:** you can explain crash recovery. **In projects:** databases.
- **Required first:** `iv-wal` (STORE 2). **Helpful:** `iv-buffer-pool` (STORE 9). **Used again in:** `iv-raft-persist` (DIST 21).
- **Cookbook:** similar\* `aries-recovery`. **Status:** to be written. **Runner:** gradable today.

#### STORE 15. Concurrent B+ tree latching

`iv-latch-crabbing` · 120 min · hard · P3 · Tracks: C1

- **New idea:** lock a child before releasing its parent.
- **Core:** Readers and inserters with latch coupling.
- **Follow-up 1:** Optimistic descent. *Test:* restart counters.
- **Follow-up 2:** Safe-node release. *Test:* max latches held.
- **Follow-up 3:** Version-based optimistic coupling. *Test:* conservation under stress.
- **Trade-offs:** Deadlock order. **Concepts:** latch coupling.
- **After this:** you can share an index between threads. **In projects:** databases.
- **Required first:** `iv-bptree-memory` (COLL 16). **Helpful:** `iv-rwlock` (CONC 17).
- **Cookbook:** similar\* `optimistic-lock-coupling-btree`. **Status:** to be written. **Runner:** ok; races need stress or loom.

#### STORE 16. Atomic multi-block writes with a redo journal

`iv-redo-journal` · 120 min · hard · P2 · prep · Tracks: STORE

- **New idea:** a checksummed commit record between two barriers makes a group of block writes all or nothing.
- **Core:** Buffer block writes in a transaction, journal header and images, sync, write the commit record, sync, checkpoint, and replay or discard on open.
- **Follow-up 1:** crash during recovery and recover again. *Test:* state is old or new at every boundary ; a stale commit record never matches a later transaction :: hand-built journal images.
- **Required first:** `iv-checksums` (BIN 12), `iv-disk-fault-model` (TEST 10).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

#### STORE 17. Log effects, not requests

`iv-effect-log` · 60 min · medium · P2 · prep · Tracks: STORE

- **New idea:** a replayable log records deterministic outcomes and transaction boundaries.
- **Core:** For a small store with relative TTLs, expiry and a pop from the first non-empty queue, emit log records that rebuild the same state at any later replay time, including when keys have since passed their deadline.
- **Follow-up 1:** replay at a later clock. *Test:* seeded operations, replay far in the future equals the original state filtered by expiry ; transactions :: cut the record list anywhere inside a marked group and the whole group is absent.
- **Required first:** `iv-wal` (STORE 2), `iv-fake-clock` (TEST 1). **Used again in:** `iv-online-log-rewrite` (STORE 19).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

#### STORE 18. Group commit and fsync policies

`iv-group-commit` · 60 min · medium · P2 · prep · Tracks: STORE

- **New idea:** many requests share one sync, and replies wait for it.
- **Core:** Make a writer loop drain queued requests, append all records once, apply an always, every-interval or never sync policy through a sink trait, and only then acknowledge.
- **Follow-up 1:** ack ordering. *Test:* a recording sink shows no acknowledgement before its sync under always and fewer syncs than requests ; write and sync failures :: failing sink makes the affected requests return errors.
- **Required first:** `iv-wal` (STORE 2), `iv-actor` (CONC 16).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

#### STORE 19. Compact a log while writes continue

`iv-online-log-rewrite` · 90 min · hard · P2 · prep · Tracks: STORE

- **New idea:** snapshot plus buffered tail equals the live log.
- **Core:** Rewrite an append-only key-value log from a point-in-time copy on a background thread while new records go to both the old log and a tail buffer, then append the tail, sync and rename over the old file.
- **Follow-up 1:** interleaving. *Test:* gate the background thread with a channel, write during the rewrite, reload and compare with a model ; failure at each step :: injected errors leave a usable log that still receives writes.
- **Required first:** `iv-atomic-file` (IO 5), `iv-effect-log` (STORE 17).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

#### STORE 20. Crash-safe ordering across a set of files

`iv-crash-ordered-files` · 90 min · hard · P2 · prep · Tracks: STORE

- **New idea:** the order of publish, truncate and delete steps decides what every crash point recovers to.
- **Core:** Over a fake filesystem that records steps, implement flush and replace-many-files operations plus startup recovery so that every crash prefix recovers correct contents.
- **Follow-up 1:** follow-up 1 inject delete failures and keep in-memory state consistent. *Test:* fail each step and compare reads with a model ; follow-up 2 add a manifest naming live files :: crash at every step, orphans removed and no data lost.
- **Required first:** `iv-atomic-file` (IO 5), `iv-wal` (STORE 2).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

### QUERY: Query engine

From expression evaluation to join ordering. *Core series, 15 parts, about 20 h. Ends in `cap-optimizer`.* Alongside it: `iv-test-differential` (TEST 6), `iv-mon-flame` (MON 7).

#### QUERY 1. Expression evaluation with NULL logic

`iv-expr-eval-3vl` · 60 min · medium · P2 · Tracks: L7

- **New idea:** NULL makes logic three valued.
- **Core:** Evaluate typed expressions with three-valued logic.
- **Follow-up 1:** Bind names to indices and type check before running. *Test:* errors without rows.
- **Follow-up 2:** Constant folding. *Test:* same results on random trees.
- **Follow-up 3:** Checked arithmetic errors. *Test:* tables.
- **Trade-offs:** Interpreter vs closures. **Concepts:** AST, 3VL, binding.
- **After this:** you can evaluate predicates. **In projects:** SQL and rule engines.
- **Required first:** `iv-pratt` (PARSE 12). **Used again in:** `iv-mini-sql-exec` (QUERY 8).
- **Cookbook:** similar `tiny-sql-engine` (binding and NULL semantics), related `relational-algebra-engine`. **Status:** to be written. **Runner:** gradable today.

#### QUERY 2. Volcano executor

`iv-volcano` · 60 min · medium · P1 · Tracks: L4

- **New idea:** operators pull rows from their children.
- **Core:** Scan, Filter, Project, Limit as pull iterators behind a trait.
- **Follow-up 1:** Sort and hash aggregate. *Test:* oracle on random plans.
- **Follow-up 2:** LIMIT stops the scan early. *Test:* rows-pulled counter.
- **Follow-up 3:** Batch-at-a-time version. *Test:* same results.
- **Trade-offs:** Pull vs push vs vectorised. **Concepts:** operator trait, iterators.
- **After this:** you can compose query plans. **In projects:** query engines.
- **Required first:** `iv-iter-adapters` (ITER 1). **Used again in:** `iv-mini-sql-exec` (QUERY 8), `iv-planner-rules` (QUERY 9), `iv-lin-column` (LIN 9).
- **Cookbook:** similar\* `volcano-vectorized-push`, similar `tiny-sql-engine`. **Status:** to be written. **Runner:** gradable today.

#### QUERY 3. External merge sort

`iv-external-sort` · 60 min · medium · P1 · Tracks: S5

- **New idea:** sort in runs, then merge them.
- **Core:** Sort data larger than a memory budget with spill runs.
- **Follow-up 1:** k-way merge with fan-in limit. *Test:* pass count formula.
- **Follow-up 2:** Replacement selection. *Test:* longer runs on random data.
- **Follow-up 3:** Top-N without full sort. *Test:* IO counter.
- **Trade-offs:** Fan-in vs passes. **Concepts:** external sorting, heaps.
- **After this:** you can process data larger than memory. **In projects:** analytics.
- **Required first:** `iv-compaction` (STORE 6). **Used again in:** `iv-hash-agg` (QUERY 4), `iv-merge-join` (QUERY 6).
- <details><summary>Links hidden until asked</summary>helpful: `iv-binary-heap` (COLL 4)</details>
- **Cookbook:** similar\* `external-sort-hash-aggregation`, similar `sorting-networks-external-sort`. **Status:** to be written. **Runner:** ok (a counted in-memory disk).

#### QUERY 4. Hash aggregation with spill

`iv-hash-agg` · 90 min · medium · P2 · Tracks: S4

- **New idea:** aggregate by partition when memory runs out.
- **Core:** group by with count, sum, min, max.
- **Follow-up 1:** Spill by partition under a budget. *Test:* oracle.
- **Follow-up 2:** Recursive partitioning and skew. *Test:* depth cap.
- **Follow-up 3:** Sort-based alternative. *Test:* same results.
- **Trade-offs:** Hash vs sort. **Concepts:** partitioning, accumulators.
- **After this:** you can group at scale. **In projects:** analytics.
- **Required first:** `iv-external-sort` (QUERY 3). **Helpful:** `iv-hashmap-chaining` (COLL 3). **Used again in:** `iv-hash-join` (QUERY 5).
- **Cookbook:** similar\* `external-sort-hash-aggregation`. **Status:** to be written. **Runner:** gradable today.

#### QUERY 5. Hash join and its spill

`iv-hash-join` · 90 min · medium · P1 · Tracks: S4

- **New idea:** build one side, probe with the other.
- **Core:** Build and probe an equi-join that returns the same multiset as nested loops.
- **Follow-up 1:** Duplicate keys and outer joins. *Test:* nested-loop oracle.
- **Follow-up 2:** Partitioned (Grace) join under a memory budget. *Test:* partition counts.
- **Follow-up 3:** Bloom filter to skip probe rows. *Test:* skipped-row counter.
- **Trade-offs:** Build side choice. **Concepts:** hash join, partitioning.
- **After this:** you can join large inputs. **In projects:** databases.
- **Required first:** `iv-hash-agg` (QUERY 4). **Helpful:** `iv-bloom` (CACHE 4).
- **Cookbook:** similar\* `join-algorithms`, similar `radix-partitioned-hash-join`. **Status:** to be written. **Runner:** gradable today.

#### QUERY 6. Sort-merge join

`iv-merge-join` · 60 min · medium · P2 · Tracks: S4

- **New idea:** walk two sorted inputs together.
- **Core:** Join two sorted inputs.
- **Follow-up 1:** Duplicate runs on both sides. *Test:* nested-loop oracle.
- **Follow-up 2:** Semi and anti joins. *Test:* oracle.
- **Follow-up 3:** Lazy iterator form. *Test:* laziness proven with counting sources.
- **Trade-offs:** When an input is already sorted. **Concepts:** merge join, peekable iterators.
- **After this:** you can join ordered data. **In projects:** databases.
- **Required first:** `iv-external-sort` (QUERY 3). **Used again in:** `iv-asof-join` (QUERY 11), `iv-interesting-orders` (QUERY 15), `iv-inverted-index` (SEARCH 1).
- **Cookbook:** similar\* `join-algorithms` (its sort-merge). **Status:** to be written. **Runner:** gradable today.

#### QUERY 7. Parse a SQL subset

`iv-sql-parser` · 120 min · medium · P2 · Tracks: S2

- **New idea:** turn query text into a syntax tree.
- **Core:** SELECT cols FROM t WHERE expr.
- **Follow-up 1:** Expressions with precedence, AND, OR, NOT. *Test:* golden trees.
- **Follow-up 2:** INSERT and CREATE TABLE. *Test:* golden.
- **Follow-up 3:** Errors with spans. *Test:* positions.
- **Trade-offs:** Keywords vs identifiers. **Concepts:** recursive descent, Pratt.
- **After this:** you can accept a query language. **In projects:** data tools.
- **Required first:** `iv-pratt` (PARSE 12). **Used again in:** `iv-mini-sql-exec` (QUERY 8).
- **Cookbook:** similar `tiny-sql-engine` (lexer and parser). **Status:** to be written. **Runner:** gradable today.

#### QUERY 8. Mini SQL over in-memory tables

`iv-mini-sql-exec` · 120 min · hard · P2 · Tracks: N

- **New idea:** bind a tree to operators and run it.
- **Core:** Run SELECT with WHERE and ORDER BY from a given AST.
- **Follow-up 1:** GROUP BY aggregates. *Test:* oracle.
- **Follow-up 2:** One join. *Test:* oracle.
- **Follow-up 3:** LIMIT with early exit. *Test:* counter.
- **Trade-offs:** Bind then execute. **Concepts:** query pipeline.
- **After this:** you can ship a tiny database. **In projects:** embedded query features.
- **Required first:** `iv-volcano` (QUERY 2). **Helpful:** `iv-sql-parser` (QUERY 7), `iv-expr-eval-3vl` (QUERY 1).
- **Cookbook:** similar\* `tiny-sql-engine`. **Status:** to be written. **Runner:** gradable today.

#### QUERY 9. Rule-based planner

`iv-planner-rules` · 90 min · medium · P2 · Tracks: L7

- **New idea:** rewrite plans without changing results.
- **Core:** Turn a logical plan into a physical plan.
- **Follow-up 1:** Predicate pushdown and projection pruning. *Test:* results unchanged on random plans, rows-processed falls.
- **Follow-up 2:** Pick index scan vs sequential scan by a cost. *Test:* chosen plan asserted.
- **Follow-up 3:** Rules to a fixpoint. *Test:* terminates.
- **Trade-offs:** Rules vs cost. **Concepts:** tree rewriting.
- **After this:** you can write optimiser rules. **In projects:** query engines.
- **Required first:** `iv-volcano` (QUERY 2). **Helpful:** `iv-constant-fold` (LANG 2). **Used again in:** `iv-join-order` (QUERY 13), `iv-interesting-orders` (QUERY 15), `iv-lin-column` (LIN 9).
- **Cookbook:** similar `query-optimizer`, similar `query-rewrite-rules-magic-sets`. **Status:** to be written. **Runner:** gradable today.

#### QUERY 10. Window functions

`iv-window-functions` · 60 min · medium · P3 · Tracks: S3

- **New idea:** aggregates over a sliding frame.
- **Core:** row_number, rank, running sum over partitions.
- **Follow-up 1:** Frames with ROWS bounds. *Test:* brute force.
- **Follow-up 2:** Min and max over frames in O(n log n). *Test:* oracle and scale.
- **Follow-up 3:** Ties in rank vs dense_rank. *Test:* tables.
- **Trade-offs:** Segment tree vs deque. **Concepts:** partitioning, sliding frames.
- **After this:** you can build analytics functions. **In projects:** reporting.
- <details><summary>Links hidden until asked</summary>required first: `iv-range-queries` (COLL 12); helpful: `iv-minstack-queue` (COLL 1)</details>
- **Cookbook:** similar\* `sql-window-functions-ctes`. **Status:** to be written. **Runner:** gradable today.

#### QUERY 11. As-of join and gap filling for time series

`iv-asof-join` · 60 min · medium · P3 · Tracks: S3

- **New idea:** match each row to the latest earlier one.
- **Core:** For each trade, the latest quote at or before it.
- **Follow-up 1:** Per-symbol and tolerance. *Test:* oracle.
- **Follow-up 2:** Downsample with gap filling. *Test:* oracle.
- **Follow-up 3:** Out-of-order input policy. *Test:* tables.
- **Trade-offs:** Merge vs binary search. **Concepts:** two-pointer merge.
- **After this:** you can join time series. **In projects:** finance and telemetry.
- **Required first:** `iv-merge-join` (QUERY 6).
- **Cookbook:** similar\* `asof-joins-gap-filling`. **Status:** to be written. **Runner:** gradable today.

#### QUERY 12. Histograms and selectivity

`iv-cardinality` · 60 min · medium · P3 · Tracks: S3

- **New idea:** estimate row counts from summaries.
- **Core:** Equi-depth histogram and range estimates.
- **Follow-up 1:** Most-common values. *Test:* error bounds on skew.
- **Follow-up 2:** Conjunctions under independence. *Test:* q-error shown on correlated data.
- **Follow-up 3:** Reservoir sample estimator. *Test:* bound.
- **Trade-offs:** Why estimates fail. **Concepts:** statistics, estimation.
- **After this:** you can read query plans. **In projects:** performance work.
- **Required first:** `iv-reservoir` (CACHE 7). **Helpful:** `iv-count-min` (CACHE 5). **Used again in:** `iv-predicate-selectivity` (QUERY 14).
- **Cookbook:** similar\* `statistics-histograms-sketches`. **Status:** to be written. **Runner:** gradable today.

#### QUERY 13. Join order by dynamic programming

`iv-join-order` · 120 min · hard · P2 · Tracks: DP

- **New idea:** dynamic programming over subsets of tables.
- **Core:** Best left-deep order for a chain of joins with given cardinalities.
- **Follow-up 1:** Bushy trees over bitmasks. *Test:* brute force.
- **Follow-up 2:** Avoid cross products. *Test:* connectivity check.
- **Follow-up 3:** Greedy heuristic comparison. *Test:* never cheaper than DP.
- **Trade-offs:** Search space growth. **Concepts:** bitmask DP, cost models.
- **After this:** you can explain cost-based optimisers. **In projects:** databases.
- **Required first:** `iv-alg-co-bitmask-dp` (COMBO 6). **Helpful:** `iv-planner-rules` (QUERY 9).
- **Cookbook:** similar\* `join-enumeration-dpccp-dphyp`, similar `query-optimizer`. **Status:** to be written. **Runner:** gradable today.

#### QUERY 14. Selectivity of a predicate tree

`iv-predicate-selectivity` · 60 min · medium · P2 · prep · Tracks: QUERY

- **New idea:** combine per-column estimates through boolean structure, with stated assumptions.
- **Core:** Given per-column estimators, estimate the kept fraction of trees with AND, OR, IN and comparisons, merging range bounds on one column first.
- **Follow-up 1:** Equi-join selectivity from distinct counts. *Test:* against true join sizes on generated key and foreign-key tables within a q-error bound ; Split conjuncts into one index range for a chosen column plus a residual :: range AND residual equals the original filter on random rows.
- **Required first:** `iv-cardinality` (QUERY 12), `iv-alg-ast-walk` (REC 2).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

#### QUERY 15. Plan choice with output orders

`iv-interesting-orders` · 90 min · hard · P2 · prep · Tracks: QUERY

- **New idea:** a costlier plan must be kept when its output order saves work later.
- **Core:** Cost nested loop, hash and merge candidates for a join, adding a Sort only when an input lacks the order, and keep the cheapest plan per output order.
- **Follow-up 1:** Three-way chain where the locally cheapest plan loses. *Test:* asserted chosen plan and total cost against exhaustive enumeration ; Treat columns equated by a join as equivalent orders :: Sort node count on fixed plans.
- **Required first:** `iv-planner-rules` (QUERY 9), `iv-merge-join` (QUERY 6).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

### TXN: Transactions

Isolation from versions to locks. *Core series, 6 parts, about 8 h. Ends in `cap-database`.* Alongside it: `iv-interleaving-checker` (ATOM 9), `iv-test-differential` (TEST 6).

#### TXN 1. Time-travelling map and snapshots

`iv-versioned-map` · 60 min · medium · P1 · Tracks: S4

- **New idea:** keep old versions so readers see the past.
- **Core:** set(k, v) returns a version; get_at(k, version).
- **Follow-up 1:** snapshot() in O(1) that later writes do not change. *Test:* many snapshots against cloned models.
- **Follow-up 2:** Delete and range scan at a version. *Test:* oracle.
- **Follow-up 3:** Compact versions no snapshot can see. *Test:* size bound.
- **Trade-offs:** Per-key version lists vs persistent trees. **Concepts:** MVCC idea, BTreeMap range, binary search.
- **After this:** you can snapshot state. **In projects:** config and MVCC.
- **Required first:** `iv-interval-booking` (COLL 10). **Used again in:** `iv-mvcc-si` (TXN 3), `iv-occ` (TXN 5).
- **Cookbook:** similar `mvcc-snapshot-isolation`, related `snapshot-cow-rcu`. **Status:** staged draft: `y12-mvcc-snapshot-isolation`. **Runner:** gradable today.

#### TXN 2. Is this history serialisable

`iv-serializability-checker` · 60 min · medium · P2 · Tracks: D

- **New idea:** a history is correct if its conflict graph has no cycle.
- **Core:** Build the precedence graph and report a serial order or a cycle.
- **Follow-up 1:** Recoverable and strict classes. *Test:* tables.
- **Follow-up 2:** View serialisability by brute force. *Test:* small schedules.
- **Follow-up 3:** Name the anomaly. *Test:* scripted anomalies.
- **Trade-offs:** Conflict vs view. **Concepts:** precedence graphs.
- **After this:** you can judge isolation. **In projects:** database testing.
- **Used again in:** `iv-mvcc-si` (TXN 3), `iv-lock-manager` (TXN 4), `iv-linearizability` (DIST 16).
- <details><summary>Links hidden until asked</summary>required first: `iv-alg-order` (GRAPH 4)</details>
- **Cookbook:** similar\* `transaction-theory-serializability`. **Status:** to be written. **Runner:** gradable today.

#### TXN 3. MVCC with snapshot isolation

`iv-mvcc-si` · 120 min · hard · P1 · Tracks: Y12

- **New idea:** snapshot reads with first committer wins.
- **Core:** begin, read, write, commit with version chains.
- **Follow-up 1:** First committer wins. *Test:* scripted interleavings.
- **Follow-up 2:** Garbage collect below the oldest snapshot. *Test:* version count bound.
- **Follow-up 3:** Write skew shown, then prevented with read-set validation. *Test:* serialisability checker.
- **Trade-offs:** SI vs serialisable. **Concepts:** versioning, conflict detection.
- **After this:** you can explain Postgres-style isolation. **In projects:** databases.
- **Required first:** `iv-versioned-map` (TXN 1). **Helpful:** `iv-serializability-checker` (TXN 2).
- **Cookbook:** similar\* `mvcc-snapshot-isolation` (version chains and first committer wins). **Status:** staged draft: `y12-mvcc-snapshot-isolation`. **Runner:** gradable today.

#### TXN 4. 2PL lock manager with deadlock detection

`iv-lock-manager` · 120 min · hard · P1 · Tracks: C1

- **New idea:** lock queues and waits-for cycles.
- **Core:** Shared and exclusive locks with FIFO queues, driven as a step machine.
- **Follow-up 1:** Upgrades. *Test:* scripted schedules.
- **Follow-up 2:** Waits-for graph cycle detection and victim choice. *Test:* random schedules always finish.
- **Follow-up 3:** Wound-wait and wait-die. *Test:* no cycles ever.
- **Trade-offs:** Detection vs prevention. **Concepts:** lock queues, graph cycles.
- **After this:** you can explain two-phase locking. **In projects:** databases.
- **Required first:** `iv-bank-transfer-concurrent` (CONC 3). **Helpful:** `iv-serializability-checker` (TXN 2). **Used again in:** `iv-occ` (TXN 5).
- <details><summary>Links hidden until asked</summary>helpful: `iv-alg-order` (GRAPH 4)</details>
- **Cookbook:** similar\* `lock-manager-deadlock`. **Status:** to be written. **Runner:** gradable today.

#### TXN 5. Optimistic concurrency control

`iv-occ` · 60 min · medium · P2 · Tracks: Y12

- **New idea:** check at commit that nothing you read changed.
- **Core:** Read set, write set, validate and commit.
- **Follow-up 1:** Backward validation. *Test:* committed histories are serialisable (checker).
- **Follow-up 2:** Retry loop. *Test:* invariants under random interleavings.
- **Follow-up 3:** Compare abort rates with 2PL. *Test:* printed.
- **Trade-offs:** Contention behaviour. **Concepts:** validation, serialisability.
- **After this:** you can use optimistic concurrency. **In projects:** APIs and stores.
- **Required first:** `iv-versioned-map` (TXN 1). **Helpful:** `iv-lock-manager` (TXN 4). **Used again in:** `iv-timestamp-ordering` (TXN 6), `iv-lin-event-store` (LIN 4).
- **Cookbook:** similar\* `timestamp-ordering-occ`. **Status:** to be written. **Runner:** gradable today.

#### TXN 6. Timestamp ordering

`iv-timestamp-ordering` · 60 min · medium · P3 · Tracks: Y12

- **New idea:** timestamps fix the serial order in advance.
- **Core:** Read and write rules with aborts.
- **Follow-up 1:** Thomas write rule. *Test:* acceptance differs on scripted schedules.
- **Follow-up 2:** Strictness with commit waits. *Test:* no dirty reads.
- **Follow-up 3:** Restart with new timestamps. *Test:* all commit eventually.
- **Trade-offs:** vs 2PL. **Concepts:** timestamps, schedules.
- **After this:** you can compare concurrency control. **In projects:** databases.
- **Required first:** `iv-occ` (TXN 5).
- **Cookbook:** similar\* `timestamp-ordering-occ`. **Status:** to be written. **Runner:** gradable today.

### SEARCH: Search and similarity

From postings to vector indexes. *Core series, 17 parts, about 20 h. Ends in `cap-search`.* Alongside it: `iv-mon-metrics` (MON 2).

#### SEARCH 1. Inverted index

`iv-inverted-index` · 60 min · medium · P1 · Tracks: S4

- **New idea:** map each term to the documents holding it.
- **Core:** add documents, AND query over terms.
- **Follow-up 1:** OR, NOT with sorted posting intersection. *Test:* scan oracle.
- **Follow-up 2:** Phrase queries with positions. *Test:* oracle.
- **Follow-up 3:** Delete and update documents. *Test:* oracle.
- **Trade-offs:** Galloping intersection. **Concepts:** posting lists, merging.
- **After this:** you can add search to an app. **In projects:** docs and logs.
- **Required first:** `iv-hashmap-chaining` (COLL 3). **Helpful:** `iv-merge-join` (QUERY 6). **Used again in:** `iv-posting-compression` (SEARCH 2), `iv-boolean-query` (SEARCH 3), `iv-trigram` (SEARCH 4), `iv-bm25` (SEARCH 5), `iv-positional-phrase` (SEARCH 16), `iv-immutable-segments` (SEARCH 17).
- **Cookbook:** similar `search-engine`, similar\* `systems-design-in-code` (its Q16). **Status:** to be written (partly practised by `y10-trigram-index`). **Runner:** gradable today.

#### SEARCH 2. Posting list compression

`iv-posting-compression` · 60 min · medium · P2 · Tracks: F7

- **New idea:** gaps between sorted ids compress well.
- **Core:** Delta plus varint with decode.
- **Follow-up 1:** Skip pointers for intersection. *Test:* decoded-count bound.
- **Follow-up 2:** Block bit packing. *Test:* round trip.
- **Follow-up 3:** next_geq cursor. *Test:* oracle.
- **Trade-offs:** Size vs decode speed. **Concepts:** delta coding.
- **After this:** you can shrink indexes. **In projects:** search engines.
- **Required first:** `iv-inverted-index` (SEARCH 1). **Helpful:** `iv-varint-codec` (BIN 1). **Used again in:** `iv-immutable-segments` (SEARCH 17).
- **Cookbook:** similar `search-engine`, related `columnar-encodings`. **Status:** exists: `f7-posting-list-codec`. **Runner:** gradable today.

#### SEARCH 3. Search query language

`iv-boolean-query` · 60 min · medium · P2 · Tracks: S2

- **New idea:** a query language over sets.
- **Core:** AND, OR, NOT, parentheses, quoted phrases.
- **Follow-up 1:** Evaluate over an inverted index. *Test:* brute-force oracle.
- **Follow-up 2:** Wildcards. *Test:* oracle.
- **Follow-up 3:** Canonicalise and print. *Test:* round trip.
- **Trade-offs:** Implicit AND. **Concepts:** parsing, set operations.
- **After this:** you can offer filters. **In projects:** log and ticket search.
- **Required first:** `iv-inverted-index` (SEARCH 1). **Helpful:** `iv-pratt` (PARSE 12).
- **Cookbook:** similar `search-engine` (boolean query parser). **Status:** to be written. **Runner:** gradable today.

#### SEARCH 4. Trigram substring search

`iv-trigram` · 60 min · medium · P2 · Tracks: Y10

- **New idea:** filter with an index, then verify.
- **Core:** Index trigrams, filter then verify.
- **Follow-up 1:** Short queries fallback. *Test:* oracle.
- **Follow-up 2:** Case folding. *Test:* Unicode cases.
- **Follow-up 3:** Extract trigrams from a regex literal. *Test:* no false negatives.
- **Trade-offs:** Selectivity. **Concepts:** filter-verify.
- **After this:** you can substring search. **In projects:** code search.
- **Required first:** `iv-inverted-index` (SEARCH 1). **Used again in:** `iv-suffix-array` (SEARCH 12), `iv-qgram-typo-filter` (SEARCH 15).
- **Cookbook:** similar\* `trigram-index`. **Status:** exists: `y10-trigram-index`. **Runner:** gradable today.

#### SEARCH 5. Ranked search with BM25

`iv-bm25` · 60 min · medium · P2 · Tracks: Y10

- **New idea:** rank by term weight and keep the best k.
- **Core:** Score and return top k.
- **Follow-up 1:** Top k with a bounded heap. *Test:* equals full sort.
- **Follow-up 2:** Updates keep statistics right. *Test:* rebuild oracle.
- **Follow-up 3:** Segments give identical scores. *Test:* split invariance.
- **Trade-offs:** idf variants. **Concepts:** scoring, top-k heap.
- **After this:** you can rank results. **In projects:** product search.
- **Required first:** `iv-inverted-index` (SEARCH 1).
- <details><summary>Links hidden until asked</summary>helpful: `iv-binary-heap` (COLL 4)</details>
- **Cookbook:** similar `search-engine`. **Status:** exists: `y10-bm25-ranking`. **Runner:** gradable today.

#### SEARCH 6. Autocomplete service core

`iv-autocomplete` · 60 min · medium · P1 · Tracks: F4

- **New idea:** cache the best completions on each prefix.
- **Core:** Top k completions by weight for a prefix.
- **Follow-up 1:** Cached top k per node with updates. *Test:* scan oracle.
- **Follow-up 2:** Decayed popularity with fake clock. *Test:* oracle.
- **Follow-up 3:** One-typo tolerance. *Test:* brute-force edit distance.
- **Trade-offs:** Memory vs latency. **Concepts:** trie, top-k, decay.
- **After this:** you can type-ahead. **In projects:** search boxes.
- **Required first:** `iv-trie` (COLL 6). **Helpful:** `iv-binary-heap` (COLL 4).
- **Cookbook:** similar\* `autocomplete-engine` (cached top-k, decay, fuzzy search), similar `typeahead-service`. **Status:** to be written. **Runner:** gradable today.

#### SEARCH 7. Spell checker with edit distance

`iv-fuzzy` · 60 min · medium · P2 · Tracks: D

- **New idea:** prune candidates by edit distance.
- **Core:** Levenshtein with a bounded band.
- **Follow-up 1:** BK-tree lookup. *Test:* brute force.
- **Follow-up 2:** Walk a trie with DP rows. *Test:* brute force and node-visit bound.
- **Follow-up 3:** Transpositions. *Test:* tables.
- **Trade-offs:** BK-tree vs automaton. **Concepts:** DP, metric trees.
- **After this:** you can spell correction. **In projects:** search and CLIs.
- **Required first:** `iv-alg-dp-strings` (DP 4). **Helpful:** `iv-trie` (COLL 6). **Used again in:** `iv-qgram-typo-filter` (SEARCH 15).
- **Cookbook:** similar\* `spatial-search-structures` (BK-tree), related `dp-edit-distance`. **Status:** to be written. **Runner:** gradable today.

#### SEARCH 8. Multi-pattern matching

`iv-aho-corasick` · 90 min · hard · P2 · Tracks: D

- **New idea:** failure links search many patterns at once.
- **Core:** Build trie and failure links, report all matches.
- **Follow-up 1:** Streaming feed. *Test:* chunk splits.
- **Follow-up 2:** Leftmost-longest mode. *Test:* oracle.
- **Follow-up 3:** Replace-all. *Test:* oracle.
- **Trade-offs:** vs many memchr. **Concepts:** automaton, failure links.
- **After this:** you can scan for many strings. **In projects:** filters and scanners.
- **Required first:** `iv-trie` (COLL 6). **Helpful:** `iv-alg-reach` (GRAPH 2).
- **Cookbook:** similar\* `aho-corasick`. **Status:** to be written. **Runner:** gradable today.

#### SEARCH 9. Brute-force vector search

`iv-knn` · 45 min · easy · P2 · Tracks: F1

- **New idea:** vector similarity with a bounded result set.
- **Core:** Top k by cosine with a bounded heap.
- **Follow-up 1:** L2 and dot, with normalisation rules. *Test:* oracle by sort.
- **Follow-up 2:** Metadata filter. *Test:* oracle.
- **Follow-up 3:** Chunked unrolled kernel. *Test:* equal within tolerance.
- **Trade-offs:** float total order. **Concepts:** distance kernels, top-k.
- **After this:** you can baseline semantic search. **In projects:** recommendations.
- **Used again in:** `iv-ivf` (SEARCH 13), `iv-hnsw` (SEARCH 14).
- <details><summary>Links hidden until asked</summary>required first: `iv-binary-heap` (COLL 4)</details>
- **Cookbook:** similar\* `knn-bruteforce`, similar `vector-distance-kernels-simd`. **Status:** to be written. **Runner:** gradable today.

#### SEARCH 10. Near-duplicate detection

`iv-lsh-dedupe` · 60 min · medium · P3 · Tracks: F4

- **New idea:** similar items collide on purpose.
- **Core:** Shingles and MinHash signatures.
- **Follow-up 1:** LSH banding candidates. *Test:* recall and precision bounds.
- **Follow-up 2:** SimHash with Hamming search. *Test:* oracle.
- **Follow-up 3:** Cluster duplicates. *Test:* union of pairs.
- **Trade-offs:** S-curve tuning. **Concepts:** hashing, similarity.
- **After this:** you can find near duplicates. **In projects:** ingest pipelines.
- **Required first:** `iv-hashmap-chaining` (COLL 3).
- <details><summary>Links hidden until asked</summary>helpful: `iv-union-find` (COLL 7)</details>
- **Cookbook:** similar\* `minhash-simhash-lsh`. **Status:** to be written. **Runner:** gradable today.

#### SEARCH 11. Spatial index

`iv-spatial-grid` · 60 min · medium · P3 · Tracks: S4

- **New idea:** index space to search nearby.
- **Core:** Grid hash radius query.
- **Follow-up 1:** k-d tree nearest. *Test:* brute force.
- **Follow-up 2:** Quadtree range. *Test:* brute force.
- **Follow-up 3:** Moving points updates. *Test:* oracle.
- **Trade-offs:** Grid vs tree. **Concepts:** spatial hashing.
- **After this:** you can find neighbours. **In projects:** maps and games.
- **Required first:** `iv-hashmap-chaining` (COLL 3).
- **Cookbook:** similar\* `spatial-search-structures`. **Status:** to be written. **Runner:** gradable today.

#### SEARCH 12. Suffix array and substring search

`iv-suffix-array` · 90 min · hard · P3 · Tracks: D

- **New idea:** sorted suffixes answer substring queries.
- **Core:** Build by sorting suffixes (prefix doubling) and search.
- **Follow-up 1:** LCP with Kasai. *Test:* naive oracle.
- **Follow-up 2:** Longest repeated substring. *Test:* oracle.
- **Follow-up 3:** BWT and FM count. *Test:* oracle.
- **Trade-offs:** Memory. **Concepts:** suffix arrays.
- **After this:** you can index genomes and code. **In projects:** search tools.
- **Required first:** `iv-trigram` (SEARCH 4).
- **Cookbook:** similar\* `string-algos`, similar `succinct-wavelet-fm-index`. **Status:** to be written. **Runner:** gradable today.

#### SEARCH 13. IVF index with k-means

`iv-ivf` · 90 min · medium · P3 · Tracks: F1

- **New idea:** search only the nearest clusters.
- **Core:** k-means partitions and nprobe search.
- **Follow-up 1:** Recall vs nprobe. *Test:* recall floor over seeds.
- **Follow-up 2:** Insert and delete. *Test:* invariants.
- **Follow-up 3:** Product quantisation codes. *Test:* recall floor.
- **Trade-offs:** Recall measurement. **Concepts:** clustering, quantisation.
- **After this:** you can trade recall for speed. **In projects:** vector databases.
- **Required first:** `iv-knn` (SEARCH 9).
- **Cookbook:** similar\* `ivf-flat`, similar `product-quantization`. **Status:** to be written. **Runner:** gradable today.

#### SEARCH 14. HNSW-lite

`iv-hnsw` · 120 min · hard · P3 · Tracks: D

- **New idea:** walk a layered neighbour graph.
- **Core:** Layered proximity graph insert and search.
- **Follow-up 1:** ef parameter. *Test:* recall floor against brute force.
- **Follow-up 2:** Deletes by tombstone. *Test:* recall.
- **Follow-up 3:** Filtered search. *Test:* oracle recall.
- **Trade-offs:** Build cost. **Concepts:** proximity graphs, best-first search.
- **After this:** you can explain modern vector indexes. **In projects:** vector databases.
- **Required first:** `iv-knn` (SEARCH 9). **Helpful:** `iv-alg-reach` (GRAPH 2).
- <details><summary>Links hidden until asked</summary>helpful: `iv-indexed-pq` (COLL 8)</details>
- **Cookbook:** similar\* `hnsw-lite`. **Status:** to be written. **Runner:** gradable today.

#### SEARCH 15. Typo candidates by shared q-grams

`iv-qgram-typo-filter` · 60 min · medium · P2 · prep · Tracks: SEARCH

- **New idea:** each edit destroys only a few q-grams, so a shared-count threshold prunes before verification.
- **Core:** Index padded trigrams, count posting hits per term, keep those above a threshold derived from the edit budget, then verify with bounded edit distance.
- **Follow-up 1:** Compare recall with a full scan for each threshold and floor. *Test:* seeded typos with listed false negatives ; Adjust padding so typos in half-typed prefixes are found :: prefix typo corpus including rsut against rustacean.
- **Required first:** `iv-trigram` (SEARCH 4), `iv-fuzzy` (SEARCH 7).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

#### SEARCH 16. Positional postings and phrase search

`iv-positional-phrase` · 60 min · medium · P2 · prep · Tracks: SEARCH

- **New idea:** keeping term positions turns a document index into a phrase matcher.
- **Core:** Index term positions per document and return documents containing a phrase at consecutive positions.
- **Follow-up 1:** follow-up 1 allow a slop of k words between terms. *Test:* compare with a brute-force token scan on seeded docs ; follow-up 2 return match start offsets for highlighting :: exact offsets on a fixed corpus.
- **Required first:** `iv-inverted-index` (SEARCH 1).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

#### SEARCH 17. Immutable index segments and merging

`iv-immutable-segments` · 90 min · hard · P2 · prep · Tracks: SEARCH

- **New idea:** new data becomes a new read-only segment and merging must be invisible to results.
- **Core:** Build segments over contiguous doc-id ranges, query across them using corpus-wide statistics, and merge adjacent segments.
- **Follow-up 1:** follow-up 1 size-tiered merge selection. *Test:* segment count bound after seeded additions ; follow-up 2 deletions with a tombstone set applied at merge :: results equal a rebuild without the deleted docs.
- **Required first:** `iv-inverted-index` (SEARCH 1), `iv-posting-compression` (SEARCH 2).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

### RESIL: Resilience patterns

Protect services from callers and dependencies. *Core series, 12 parts, about 13 h. Ends in `cap-ratelimitlab`.* Alongside it: `iv-mon-slo` (MON 5), `iv-mon-metrics` (MON 2).

#### RESIL 1. Retry with backoff and jitter

`iv-retry-backoff` · 45 min · easy · P1 · Tracks: B5

- **New idea:** wait longer each time, with jitter.
- **Core:** Delays with cap and full jitter from a seeded rng.
- **Follow-up 1:** Retry only retryable errors with a deadline. *Test:* fake sleeper log.
- **Follow-up 2:** Token retry budget. *Test:* volume bound during outage.
- **Follow-up 3:** Honour retry-after. *Test:* scripted server.
- **Trade-offs:** Thundering herds. **Concepts:** backoff, injected sleeper.
- **After this:** you can retry without stampedes. **In projects:** every client.
- **Required first:** nothing. **Helpful:** `iv-fake-clock` (TEST 1). **Used again in:** `iv-notification` (PAT 10), `iv-async-retry-hedge` (ASYNC 12), `iv-webhook-sender` (WEB 6), `iv-sbx-supervisor` (SBX 6), `iv-circuit-breaker` (RESIL 3).
- **Cookbook:** exact `retry-backoff-jitter` (delay function), similar `error-chain-retryable`. **Status:** to be written. **Runner:** gradable today.

#### RESIL 2. Rate limiter with a fake clock

`iv-rate-limiter` · 120 min · medium · P1 · Tracks: M2

- **New idea:** refill permission over time.
- **Core:** Token bucket: capacity and refill rate, time passed in, exact integer refill.
- **Follow-up 1:** Several algorithms behind one trait (sliding log, then GCRA). *Test:* differential against brute force.
- **Follow-up 2:** Per-key limits with idle eviction. *Test:* size bound.
- **Follow-up 3:** Retry-after for a rejected request. *Test:* allowed at that time, not one tick earlier.
- **Trade-offs:** Burst vs smoothness; memory per key. **Concepts:** token bucket, injected clock, integer arithmetic.
- **After this:** you can protect an API. **In projects:** gateways.
- **Required first:** `iv-fake-clock` (TEST 1). **Helpful:** `iv-hit-counter` (CACHE 3), `iv-ttl-kv` (CACHE 2). **Used again in:** `iv-notification` (PAT 10), `iv-gcra-equivalence` (RESIL 11).
- **Cookbook:** exact `token-bucket` (floating-point bucket with injected clock), similar `rate-limiter-lab` (six algorithms, keyed eviction, no retry-after), similar `systems-design-in-code` (its Q2), similar `concurrency-interview-questions` (its Q2). **Status:** staged draft `m2-rate-limiter` (new in build-your-own). **Runner:** gradable today.

#### RESIL 3. Circuit breaker

`iv-circuit-breaker` · 60 min · medium · P1 · Tracks: B5

- **New idea:** stop calling what is failing.
- **Core:** closed, open, half-open with fake clock.
- **Follow-up 1:** Sliding error-rate window. *Test:* scripted calls.
- **Follow-up 2:** Limited half-open probes under concurrency. *Test:* probe count.
- **Follow-up 3:** As a wrapper around any closure or service. *Test:* error passthrough.
- **Trade-offs:** Thresholds and flapping. **Concepts:** state machine, sliding window.
- **After this:** you can fail fast. **In projects:** service clients.
- **Required first:** `iv-retry-backoff` (RESIL 1). **Helpful:** `iv-hit-counter` (CACHE 3), `iv-vending-machine` (PAT 1). **Used again in:** `iv-async-retry-hedge` (ASYNC 12), `iv-load-balancer` (RESIL 5).
- **Cookbook:** similar `tower-service-layer` (has a circuit breaker), similar `http-client-resilience`. **Status:** to be written. **Runner:** gradable today.

#### RESIL 4. Idempotency keys

`iv-idempotency-store` · 60 min · medium · P1 · Tracks: B5

- **New idea:** a key makes a retried request harmless.
- **Core:** begin, complete, replay the stored response.
- **Follow-up 1:** Concurrent duplicates wait or conflict. *Test:* 16 threads, one execution.
- **Follow-up 2:** Payload mismatch and TTL. *Test:* fake clock.
- **Follow-up 3:** Consumer dedup window. *Test:* memory bound.
- **Trade-offs:** Where to store the key. **Concepts:** state per key, TTL.
- **After this:** you can make writes safe to retry. **In projects:** payment APIs.
- **Required first:** `iv-ttl-kv` (CACHE 2). **Helpful:** `iv-singleflight` (CONC 11). **Used again in:** `iv-outbox` (RESIL 7), `iv-saga` (RESIL 8), `iv-dist-delivery` (DIST 2), `iv-raft-apply` (DIST 20), `iv-lin-projections` (LIN 5).
- **Cookbook:** similar\* `idempotency-dedup-leader-fencing-store`, similar `axum-idempotency`. **Status:** to be written. **Runner:** gradable today.

#### RESIL 5. Load balancing strategies

`iv-load-balancer` · 60 min · medium · P1 · Tracks: B2

- **New idea:** choose a backend from load and health.
- **Core:** Round robin, weighted and least-connections behind a trait.
- **Follow-up 1:** Smooth weighted round robin. *Test:* exact sequence.
- **Follow-up 2:** Health checks and ejection on a fake clock. *Test:* scripted.
- **Follow-up 3:** Power of two choices and consistent hashing. *Test:* balance bounds.
- **Trade-offs:** Stale load data. **Concepts:** strategy, RAII in-flight guard.
- **After this:** you can balance traffic. **In projects:** proxies.
- **Required first:** `iv-circuit-breaker` (RESIL 3). **Helpful:** `iv-consistent-hash` (DIST 8).
- **Cookbook:** similar `api-gateway-service-discovery`, similar `reverse-proxy-load-balancer`. **Status:** to be written. **Runner:** gradable today.

#### RESIL 6. Service registry with TTL

`iv-service-registry` · 60 min · medium · P2 · Tracks: B5

- **New idea:** membership by expiring registrations.
- **Core:** register, heartbeat, lookup with expiry.
- **Follow-up 1:** Watch changes. *Test:* event order.
- **Follow-up 2:** Versioned snapshots. *Test:* consistent reads.
- **Follow-up 3:** Self-preservation when many expire at once. *Test:* scripted.
- **Trade-offs:** Push vs pull. **Concepts:** TTL, watch.
- **After this:** you can discover services. **In projects:** platforms.
- **Required first:** `iv-ttl-kv` (CACHE 2). **Helpful:** `iv-pubsub` (CONC 12).
- **Cookbook:** similar `api-gateway-service-discovery`. **Status:** to be written. **Runner:** gradable today.

#### RESIL 7. Transactional outbox and inbox

`iv-outbox` · 60 min · medium · P2 · Tracks: B5

- **New idea:** state and message committed together, then relayed.
- **Core:** State change and outbox row atomically; relay publishes.
- **Follow-up 1:** At-least-once relay with lease. *Test:* crash injection.
- **Follow-up 2:** Inbox dedup. *Test:* effect applied once.
- **Follow-up 3:** Ordering per aggregate. *Test:* property.
- **Trade-offs:** Polling vs tailing. **Concepts:** outbox, dedup.
- **After this:** you can publish events reliably. **In projects:** microservices.
- **Required first:** `iv-idempotency-store` (RESIL 4). **Helpful:** `iv-bank-ledger` (PAT 6). **Used again in:** `iv-saga` (RESIL 8), `iv-cdc` (DIST 14).
- **Cookbook:** similar `transactional-outbox-inbox-sqlite`. **Status:** to be written. **Runner:** ok (in-memory store; real SQL needs a database crate).

#### RESIL 8. Saga orchestrator

`iv-saga` · 60 min · medium · P2 · Tracks: B5

- **New idea:** undo by compensation.
- **Core:** Steps with compensations, reverse order on failure.
- **Follow-up 1:** Durable log and resume. *Test:* crash at every point.
- **Follow-up 2:** Retryable vs pivot steps. *Test:* ordering rules enforced.
- **Follow-up 3:** Idempotent participants. *Test:* duplicate calls.
- **Trade-offs:** Isolation anomalies. **Concepts:** compensation, durable state machine.
- **After this:** you can run multi-service workflows. **In projects:** orders and bookings.
- **Required first:** `iv-vending-machine` (PAT 1). **Helpful:** `iv-idempotency-store` (RESIL 4), `iv-outbox` (RESIL 7).
- **Cookbook:** similar\* `saga-orchestrator`. **Status:** to be written. **Runner:** gradable today.

#### RESIL 9. Time-ordered id generator

`iv-snowflake` · 45 min · easy · P2 · Tracks: S10

- **New idea:** ids made of time, node and sequence.
- **Core:** 64-bit ids from time, node and sequence.
- **Follow-up 1:** Clock going backwards. *Test:* error or wait policy.
- **Follow-up 2:** Lock-free next(). *Test:* uniqueness across threads.
- **Follow-up 3:** Sortable text form. *Test:* order property.
- **Trade-offs:** Bit budget. **Concepts:** bit packing, atomics.
- **After this:** you can generate ids without coordination. **In projects:** distributed apps.
- **Required first:** `iv-alg-bit-fields` (BIT 2). **Helpful:** `iv-atomic-counters` (MON 1).
- **Cookbook:** exact `systems-design-in-code` (its Q12). **Status:** to be written. **Runner:** gradable today.

#### RESIL 10. Fixed window, sliding log and sliding counter

`iv-window-limiters` · 60 min · medium · P2 · prep · Tracks: RESIL

- **New idea:** window-based admission trades memory for accuracy at the boundaries.
- **Core:** Implement the three window limiters behind one trait and measure the most admissions in any sliding window.
- **Follow-up 1:** follow-up 1 weighted request costs. *Test:* exact decisions on hand traces ; follow-up 2 bucketed sliding log with bounded memory :: error bound against the exact log on seeded traces.
- **Required first:** `iv-fake-clock` (TEST 1).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

#### RESIL 11. Leaky bucket meter and GCRA

`iv-gcra-equivalence` · 45 min · medium · P2 · prep · Tracks: RESIL

- **New idea:** one timestamp can encode the whole token-bucket state.
- **Core:** Implement the leaky meter and GCRA in integer time and show identical decisions to a token bucket.
- **Follow-up 1:** follow-up 1 return retry-after. *Test:* retrying at that time succeeds and one tick earlier fails ; follow-up 2 variable cost per request :: differential against the token bucket on seeded traces.
- **Required first:** `iv-rate-limiter` (RESIL 2).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

#### RESIL 12. Adaptive in-flight limiter

`iv-aimd-concurrency` · 90 min · hard · P2 · prep · Tracks: RESIL

- **New idea:** a CAS-guarded in-flight count with an AIMD limit adapts admission to capacity.
- **Core:** Build a lock-free try_acquire with RAII permits and raise the limit by one on success and halve it on overload.
- **Follow-up 1:** follow-up 1 make the limit read safe against concurrent lowering. *Test:* scripted interleaving through a test hook ; follow-up 2 latency-gradient limit rule :: deterministic round-based backend simulation converges within bounds.
- **Required first:** `iv-semaphore` (CONC 5).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

### DIST: Distributed systems to consensus

Everything on a seeded simulator: no sockets, no real time. *Core series, 23 parts, about 33 h. Ends in `cap-raftkv`.* Alongside it: `iv-sim-network` (DIST 1), `iv-mon-spans` (MON 4), `iv-linearizability` (DIST 16).

#### DIST 1. Deterministic network simulator

`iv-sim-network` · 90 min · hard · P2 · Tracks: Y5

- **New idea:** a seeded event queue makes failures repeatable.
- **Core:** Nodes, messages and timers on a seeded event queue.
- **Follow-up 1:** Loss, delay, partition. *Test:* replay equality from a seed.
- **Follow-up 2:** Invariant hooks and trace. *Test:* planted bug found.
- **Follow-up 3:** Shrink a fault schedule. *Test:* still failing.
- **Trade-offs:** What determinism requires. **Concepts:** DES, seeds.
- **After this:** you can test any protocol deterministically. **In projects:** distributed testing.
- **Helpful:** `iv-fake-clock` (TEST 1). **Used again in:** `iv-dist-delivery` (DIST 2), `iv-dist-heartbeat` (DIST 4), `iv-lamport-vclock` (DIST 6), `iv-two-phase-commit` (DIST 15), `iv-raft-election` (DIST 18), `iv-crdt-counters` (CRDT 1), `iv-crdt-convergence` (CRDT 4), `iv-sim-invariant-search` (TEST 12).
- <details><summary>Links hidden until asked</summary>required first: `iv-job-scheduler` (CONC 15)</details>
- **Cookbook:** similar\* `deterministic-simulation-testing`. **Status:** to be written. **Runner:** ok (turmoil is vendored for the tokio variant).

#### DIST 2. Delivery guarantees over a lossy link

`iv-dist-delivery` · 60 min · medium · P1 · Tracks: Y13

- **New idea:** retries create duplicates, so effects must dedup.
- **Core:** On a seeded simulated link, build at-most-once and at-least-once delivery with acknowledgements.
- **Follow-up 1:** Deduplicate to get an exactly-once effect. *Test:* effect count equals send count.
- **Follow-up 2:** Bounded dedup window and what escapes it. *Test:* scripted late duplicate.
- **Follow-up 3:** Per-sender FIFO. *Test:* order property under reordering.
- **Trade-offs:** Exactly-once delivery vs exactly-once effect. **Concepts:** acknowledgements, retransmission, dedup.
- **After this:** you can name delivery guarantees. **In projects:** queues and webhooks.
- **Required first:** `iv-sim-network` (DIST 1). **Helpful:** `iv-idempotency-store` (RESIL 4). **Used again in:** `iv-reliable-udp` (NET 3), `iv-visibility-queue` (DIST 3), `iv-crdt-delta` (CRDT 8), `iv-crdt-opbased` (CRDT 9).
- **Cookbook:** similar `udp-sockets-reliable`, similar `idempotency-dedup-leader-fencing-store`. **Status:** to be written. **Runner:** gradable today.

#### DIST 3. Message queue with at-least-once delivery

`iv-visibility-queue` · 60 min · medium · P1 · Tracks: Y13

- **New idea:** a message returns unless acknowledged in time.
- **Core:** send, receive with a visibility timeout, ack.
- **Follow-up 1:** Dead letter after N deliveries. *Test:* counts.
- **Follow-up 2:** Stale receipt rejection. *Test:* scripted.
- **Follow-up 3:** Per-group FIFO. *Test:* order property under redelivery.
- **Trade-offs:** Exactly-once is a myth. **Concepts:** visibility timeout, receipts.
- **After this:** you can build a work queue. **In projects:** job systems.
- **Required first:** `iv-dist-delivery` (DIST 2). **Helpful:** `iv-job-scheduler` (CONC 15).
- **Cookbook:** exact `systems-design-in-code` (its Q5: timeout, receipts, dead letters). **Status:** to be written. **Runner:** gradable today.

#### DIST 4. Failure detection by heartbeat

`iv-dist-heartbeat` · 60 min · medium · P1 · Tracks: Y13

- **New idea:** silence can only raise suspicion.
- **Core:** Suspect a peer after missed heartbeats on a simulated clock.
- **Follow-up 1:** Adaptive timeout from arrival history. *Test:* fewer false positives on jittery traces.
- **Follow-up 2:** Accrual-style suspicion level. *Test:* monotonic while silent.
- **Follow-up 3:** Partition vs crash look the same. *Test:* scripted.
- **Trade-offs:** Completeness vs accuracy. **Concepts:** timeouts, statistics.
- **After this:** you can tune failure detection. **In projects:** clusters.
- **Required first:** `iv-sim-network` (DIST 1). **Used again in:** `iv-gossip-failure` (DIST 5), `iv-lease-fencing` (DIST 7), `iv-raft-election` (DIST 18).
- **Cookbook:** related `swim-gossip`. **Status:** to be written. **Runner:** gradable today.

#### DIST 5. Failure detector with gossip

`iv-gossip-failure` · 120 min · hard · P3 · Tracks: Y13

- **New idea:** spread membership by random peers.
- **Core:** Ping and suspect on a simulated network.
- **Follow-up 1:** Indirect pings. *Test:* false-positive reduction.
- **Follow-up 2:** Incarnation numbers refute suspicion. *Test:* scripted.
- **Follow-up 3:** Dissemination convergence bound. *Test:* seeds.
- **Trade-offs:** Detection time vs false positives. **Concepts:** SWIM.
- **After this:** you can run decentralised membership. **In projects:** clusters.
- **Required first:** `iv-dist-heartbeat` (DIST 4).
- **Cookbook:** similar\* `swim-gossip`. **Status:** to be written. **Runner:** gradable today.

#### DIST 6. Logical clocks and causal delivery

`iv-lamport-vclock` · 60 min · medium · P2 · Tracks: Y13

- **New idea:** order events by causality, not by wall time.
- **Core:** Lamport and vector clocks with compare.
- **Follow-up 1:** Causal broadcast buffer. *Test:* random delivery orders respect causality.
- **Follow-up 2:** Sibling detection in a KV. *Test:* concurrent writes kept.
- **Follow-up 3:** Hybrid logical clock. *Test:* monotonic under skew.
- **Trade-offs:** Size vs precision. **Concepts:** causality.
- **After this:** you can detect concurrent writes. **In projects:** replicated stores.
- **Required first:** `iv-sim-network` (DIST 1). **Used again in:** `iv-quorum-kv` (DIST 10), `iv-raft-core` (DIST 17), `iv-crdt-registers` (CRDT 2), `iv-crdt-causal-context` (CRDT 5), `iv-crdt-opbased` (CRDT 9).
- **Cookbook:** similar\* `vector-clocks-hlc`. **Status:** to be written. **Runner:** gradable today.

#### DIST 7. Lease service with fencing tokens

`iv-lease-fencing` · 60 min · medium · P1 · Tracks: Y13

- **New idea:** a lease needs a token the resource checks.
- **Core:** acquire, renew, release on a fake clock returning rising tokens.
- **Follow-up 1:** Storage rejects stale tokens. *Test:* seeded schedules with pauses.
- **Follow-up 2:** Leader election on top. *Test:* at most one valid leader per token.
- **Follow-up 3:** Compare without fencing. *Test:* violation found by search.
- **Trade-offs:** Why leases alone fail. **Concepts:** leases, fencing, simulation.
- **After this:** you can elect and fence leaders. **In projects:** coordination services.
- **Required first:** `iv-dist-heartbeat` (DIST 4). **Helpful:** `iv-ttl-kv` (CACHE 2). **Used again in:** `iv-dist-sharding` (DIST 9), `iv-raft-reads` (DIST 23).
- **Cookbook:** similar\* `lease-fencing-sim`, similar `systems-design-in-code` (its Q11). **Status:** to be written. **Runner:** gradable today.

#### DIST 8. Consistent hashing

`iv-consistent-hash` · 60 min · medium · P1 · Tracks: F4

- **New idea:** hash keys and nodes to one ring.
- **Core:** Ring with virtual nodes.
- **Follow-up 1:** Replicas on distinct nodes. *Test:* property.
- **Follow-up 2:** Movement bound on add and remove. *Test:* key movement counts.
- **Follow-up 3:** Rendezvous and jump hash behind one trait. *Test:* same suite, balance bounds.
- **Trade-offs:** Weighting. **Concepts:** BTreeMap ring, hashing.
- **After this:** you can spread data with little movement. **In projects:** caches and shards.
- **Required first:** nothing. **Helpful:** `iv-interval-booking` (COLL 10). **Used again in:** `iv-load-balancer` (RESIL 5), `iv-dist-sharding` (DIST 9), `iv-quorum-kv` (DIST 10), `iv-partitioned-log` (DIST 12).
- **Cookbook:** exact `consistent-hashing` (ring with virtual nodes), similar `hash-partitioning`. **Status:** exists: `f4-consistent-hash-ring`. **Runner:** gradable today.

#### DIST 9. Rebalancing a sharded store

`iv-dist-sharding` · 90 min · medium · P3 · Tracks: Y13

- **New idea:** moving a shard needs a fence and an epoch.
- **Core:** Route keys to shards and move shards between nodes.
- **Follow-up 1:** Handoff with a write fence. *Test:* no lost or duplicated write.
- **Follow-up 2:** Stale router retries after redirect. *Test:* bounded retries.
- **Follow-up 3:** Hot shard split. *Test:* model equality.
- **Trade-offs:** Hash vs range partitioning. **Concepts:** routing epochs, fencing.
- **After this:** you can reshard live data. **In projects:** sharded stores.
- **Required first:** `iv-consistent-hash` (DIST 8). **Helpful:** `iv-lease-fencing` (DIST 7).
- **Cookbook:** related `hash-partitioning`, related `distributed-query-shuffle-broadcast-join`. **Status:** to be written. **Runner:** gradable today.

#### DIST 10. Quorum KV in a simulator

`iv-quorum-kv` · 120 min · hard · P2 · Tracks: Y13

- **New idea:** overlapping read and write sets.
- **Core:** N, R, W reads and writes on simulated replicas.
- **Follow-up 1:** Read repair. *Test:* replicas converge after reads.
- **Follow-up 2:** Hinted handoff under partition. *Test:* no lost acked write.
- **Follow-up 3:** Version vectors with siblings. *Test:* concurrent writes preserved.
- **Trade-offs:** Sloppy quorums. **Concepts:** quorums, versioning.
- **After this:** you can tune consistency. **In projects:** Dynamo-style stores.
- **Required first:** `iv-lamport-vclock` (DIST 6). **Helpful:** `iv-consistent-hash` (DIST 8). **Used again in:** `iv-dist-anti-entropy` (DIST 11).
- **Cookbook:** similar\* `quorum-replication-dynamo`. **Status:** to be written. **Runner:** gradable today.

#### DIST 11. Anti-entropy with hash trees

`iv-dist-anti-entropy` · 90 min · medium · P3 · Tracks: Y13

- **New idea:** compare hash trees to find differences.
- **Core:** Two replicas find differing keys by comparing a hash tree.
- **Follow-up 1:** Exchange only the differing ranges. *Test:* bytes-sent bound.
- **Follow-up 2:** Concurrent writes during sync. *Test:* converges in later rounds.
- **Follow-up 3:** Pick winners by version. *Test:* oracle.
- **Trade-offs:** Tree shape and granularity. **Concepts:** Merkle trees, reconciliation.
- **After this:** you can repair replicas. **In projects:** storage clusters.
- **Required first:** `iv-quorum-kv` (DIST 10). **Helpful:** `iv-merkle` (LIN 2).
- **Cookbook:** similar `merkle-tree` (anti-entropy section). **Status:** to be written. **Runner:** ok (sha2).

#### DIST 12. Mini Kafka log

`iv-partitioned-log` · 120 min · medium · P2 · Tracks: Y13

- **New idea:** ordered partitions consumed by offset.
- **Core:** Append to partitions by key, read by offset.
- **Follow-up 1:** Consumer groups with assignment and committed offsets. *Test:* rebalance keeps coverage.
- **Follow-up 2:** Idempotent producer by sequence number. *Test:* duplicates dropped.
- **Follow-up 3:** Segments, retention and compaction. *Test:* model.
- **Trade-offs:** Ordering per partition. **Concepts:** log, offsets, rebalancing.
- **After this:** you can explain Kafka. **In projects:** event pipelines.
- **Required first:** `iv-wal` (STORE 2). **Helpful:** `iv-consistent-hash` (DIST 8). **Used again in:** `iv-stream-windows` (DIST 13), `iv-cdc` (DIST 14).
- **Cookbook:** similar\* `kafka-log-model`. **Status:** to be written. **Runner:** gradable today.

#### DIST 13. Event-time windows with watermarks

`iv-stream-windows` · 90 min · medium · P2 · Tracks: Y13

- **New idea:** event time progresses by watermarks.
- **Core:** Tumbling windows closed by a watermark.
- **Follow-up 1:** Late data policy. *Test:* batch oracle.
- **Follow-up 2:** Sliding and session windows. *Test:* oracle.
- **Follow-up 3:** Checkpoint and restore. *Test:* exactly-once results.
- **Trade-offs:** Latency vs completeness. **Concepts:** windows, watermarks.
- **After this:** you can aggregate late data. **In projects:** stream processing.
- **Required first:** `iv-partitioned-log` (DIST 12).
- <details><summary>Links hidden until asked</summary>helpful: `iv-job-scheduler` (CONC 15)</details>
- **Cookbook:** similar\* `stream-processing-windows`. **Status:** to be written. **Runner:** gradable today.

#### DIST 14. Change data capture consumer

`iv-cdc` · 45 min · medium · P3 · Tracks: Y13

- **New idea:** a commit log is a change feed.
- **Core:** Decode a transaction log in commit order.
- **Follow-up 1:** Replication slot retention. *Test:* retained bytes.
- **Follow-up 2:** Snapshot then stream without gaps. *Test:* model.
- **Follow-up 3:** Idempotent apply. *Test:* redelivery.
- **Trade-offs:** Slot bloat. **Concepts:** log decoding.
- **After this:** you can sync systems from a database. **In projects:** pipelines.
- **Required first:** `iv-partitioned-log` (DIST 12). **Helpful:** `iv-outbox` (RESIL 7).
- **Cookbook:** similar\* `cdc-logical-replication-log`. **Status:** to be written. **Runner:** gradable today.

#### DIST 15. Two-phase commit with crashes

`iv-two-phase-commit` · 120 min · hard · P2 · Tracks: Y13

- **New idea:** all commit or none, and it can block.
- **Core:** Coordinator and participants as step functions.
- **Follow-up 1:** Durable logs and recovery. *Test:* crash at every step, never disagree.
- **Follow-up 2:** Blocking case demonstrated. *Test:* scripted.
- **Follow-up 3:** Presumed abort. *Test:* fewer log writes.
- **Trade-offs:** 2PC vs saga. **Concepts:** atomic commit.
- **After this:** you can explain distributed transactions. **In projects:** databases.
- **Required first:** `iv-wal` (STORE 2). **Helpful:** `iv-sim-network` (DIST 1).
- **Cookbook:** similar\* `two-phase-commit`. **Status:** to be written. **Runner:** gradable today.

#### DIST 16. Linearizability checker

`iv-linearizability` · 120 min · hard · P3 · Tracks: Y5

- **New idea:** search for a legal order of overlapping operations.
- **Core:** Check a register history by search.
- **Follow-up 1:** Memoisation. *Test:* same verdicts.
- **Follow-up 2:** Queue and KV models. *Test:* scripted violations.
- **Follow-up 3:** Minimal counterexample. *Test:* still fails.
- **Trade-offs:** Complexity. **Concepts:** search, models.
- **After this:** you can check consistency claims. **In projects:** distributed testing.
- **Required first:** `iv-serializability-checker` (TXN 2). **Helpful:** `iv-alg-config-matrix` (REC 3). **Used again in:** `iv-raft-reads` (DIST 23).
- **Cookbook:** similar\* `linearizability-checker`. **Status:** to be written. **Runner:** gradable today.

#### DIST 17. Raft rules as pure functions

`iv-raft-core` · 90 min · hard · P2 · Tracks: Y7

- **New idea:** safety rules as pure functions.
- **Core:** Vote and append-entries handlers.
- **Follow-up 1:** Leader commit rule. *Test:* Figure 8 case.
- **Follow-up 2:** Random simulated elections. *Test:* election safety checker.
- **Follow-up 3:** Log matching. *Test:* checker.
- **Trade-offs:** Pure core vs I/O. **Concepts:** consensus.
- **After this:** you can review consensus code. **In projects:** replicated services.
- **Required first:** `iv-lamport-vclock` (DIST 6). **Used again in:** `iv-raft-election` (DIST 18).
- **Cookbook:** similar `raft-consensus`. **Status:** exists: `y7-raft-vote-rules`, `y7-raft-append-entries`, `y7-raft-leader-commit`. **Runner:** gradable today.

#### DIST 18. Raft 1: leader election in a simulator

`iv-raft-election` · 90 min · hard · P1 · Tracks: Y7

- **New idea:** terms and majorities elect one leader.
- **Core:** Timers, terms and votes on a seeded simulated network.
- **Follow-up 1:** At most one leader per term. *Test:* checker over 500 seeds.
- **Follow-up 2:** A new leader appears after a partition heals. *Test:* bounded rounds.
- **Follow-up 3:** Votes survive a restart. *Test:* crash injection.
- **Trade-offs:** Randomised timeouts. **Concepts:** terms, majority, timers.
- **After this:** you can build leader election. **In projects:** clusters.
- **Required first:** `iv-raft-core` (DIST 17). **Helpful:** `iv-sim-network` (DIST 1), `iv-dist-heartbeat` (DIST 4). **Used again in:** `iv-raft-replication` (DIST 19).
- **Cookbook:** similar `raft-consensus`. **Status:** to be written (partly practised by `y7-raft-vote-rules`). **Runner:** gradable today.

#### DIST 19. Raft 2: log replication

`iv-raft-replication` · 120 min · hard · P1 · Tracks: Y7

- **New idea:** the leader repairs follower logs.
- **Core:** Leader sends entries and repairs followers' logs.
- **Follow-up 1:** Log matching property. *Test:* checker after every event.
- **Follow-up 2:** Fast back-up over conflicting terms. *Test:* message-count bound.
- **Follow-up 3:** Minority partition cannot commit. *Test:* checker.
- **Trade-offs:** Backtracking cost. **Concepts:** log matching, next index.
- **After this:** you can replicate a log. **In projects:** databases.
- **Required first:** `iv-raft-election` (DIST 18). **Helpful:** `iv-wal` (STORE 2). **Used again in:** `iv-raft-apply` (DIST 20).
- **Cookbook:** similar `raft-consensus`. **Status:** to be written (partly practised by `y7-raft-append-entries`). **Runner:** gradable today.

#### DIST 20. Raft 3: commit and apply to a state machine

`iv-raft-apply` · 60 min · hard · P2 · Tracks: Y7

- **New idea:** only committed entries reach the state machine.
- **Core:** Advance the commit index and apply in order to a KV store.
- **Follow-up 1:** The previous-term commit rule. *Test:* Figure 8 schedule.
- **Follow-up 2:** Client request ids make retries safe. *Test:* duplicate submissions.
- **Follow-up 3:** All replicas apply the same sequence. *Test:* checker.
- **Trade-offs:** Where deduplication belongs. **Concepts:** commit rule, state machine replication.
- **After this:** you can build a replicated store. **In projects:** coordination services.
- **Required first:** `iv-raft-replication` (DIST 19). **Helpful:** `iv-idempotency-store` (RESIL 4). **Used again in:** `iv-raft-persist` (DIST 21), `iv-raft-reads` (DIST 23).
- **Cookbook:** similar `raft-consensus`. **Status:** to be written (partly practised by `y7-raft-leader-commit`). **Runner:** gradable today.

#### DIST 21. Raft 4: persistence and crashes

`iv-raft-persist` · 90 min · hard · P2 · Tracks: Y7

- **New idea:** what must be on disk before replying.
- **Core:** Persist term, vote and log through a storage trait and recover.
- **Follow-up 1:** Crash between any two writes. *Test:* safety checker.
- **Follow-up 2:** Torn last record. *Test:* recovery.
- **Follow-up 3:** Count syncs per commit. *Test:* bound.
- **Trade-offs:** What must be durable before replying. **Concepts:** durability, recovery.
- **After this:** you can make consensus survive crashes. **In projects:** databases.
- **Required first:** `iv-raft-apply` (DIST 20). **Helpful:** `iv-checkpoint-recovery` (STORE 14). **Used again in:** `iv-raft-snapshot` (DIST 22).
- **Cookbook:** similar `raft-consensus`, similar `deterministic-simulation-testing`. **Status:** to be written. **Runner:** gradable today.

#### DIST 22. Raft 5: snapshots

`iv-raft-snapshot` · 90 min · hard · P3 · Tracks: Y7

- **New idea:** replace old log with state.
- **Core:** Compact the log behind a snapshot.
- **Follow-up 1:** Install snapshot on a lagging follower. *Test:* converges.
- **Follow-up 2:** Snapshot racing with appends. *Test:* checker.
- **Follow-up 3:** Bounded log size. *Test:* invariant.
- **Trade-offs:** When to snapshot. **Concepts:** log compaction.
- **After this:** you can bound a replicated log. **In projects:** databases.
- **Required first:** `iv-raft-persist` (DIST 21).
- **Cookbook:** related `raft-consensus`. **Status:** to be written. **Runner:** gradable today.

#### DIST 23. Raft 6: linearizable reads

`iv-raft-reads` · 90 min · hard · P3 · Tracks: Y7

- **New idea:** reads need proof of leadership.
- **Core:** Serve reads only after confirming leadership.
- **Follow-up 1:** A deposed leader must not serve stale reads. *Test:* history checker.
- **Follow-up 2:** Lease reads on a fake clock with bounded skew. *Test:* violation found when the bound is broken.
- **Follow-up 3:** Batch read confirmations. *Test:* message bound.
- **Trade-offs:** Read index vs leases. **Concepts:** linearizability, leases.
- **After this:** you can serve consistent reads. **In projects:** coordination services.
- **Required first:** `iv-raft-apply` (DIST 20). **Helpful:** `iv-linearizability` (DIST 16), `iv-lease-fencing` (DIST 7).
- **Cookbook:** related `raft-consensus`. **Status:** to be written. **Runner:** gradable today.

### CRDT: CRDTs: replicas that converge

Merge laws first, then sets, registers, maps and sequences. Delivery orders are seeded. *Core series, 15 parts, about 23 h. Ends in `cap-collab`.* Alongside it: `iv-crdt-convergence` (CRDT 4), `iv-sim-network` (DIST 1).

#### CRDT 1. Counters that converge

`iv-crdt-counters` · 45 min · medium · P1 · Tracks: Y13

- **New idea:** merge must be commutative, associative and idempotent.
- **Core:** A grow-only counter and a PN-counter for replicas that merge in any order.
- **Follow-up 1:** Merge laws checked on random states: commutative, associative, idempotent. *Test:* generated triples.
- **Follow-up 2:** Random gossip schedules with duplicates converge to the true total. *Test:* seeded delivery orders against a central tally.
- **Follow-up 3:** Explain why a single integer with max or sum fails. *Test:* two wrong designs the tests must reject.
- **Trade-offs:** State size grows with replica count. **Concepts:** join semilattice, merge laws.
- **After this:** you can count across offline replicas. **In projects:** metrics and likes.
- **Required first:** nothing. **Helpful:** `iv-sim-network` (DIST 1). **Used again in:** `iv-crdt-registers` (CRDT 2), `iv-crdt-set` (CRDT 3).
- **Cookbook:** similar\* `crdts` (G-counter and PN-counter with law tests). **Status:** to be written. **Runner:** gradable today.

#### CRDT 2. Registers: last writer wins and multi-value

`iv-crdt-registers` · 60 min · medium · P1 · Tracks: Y13

- **New idea:** concurrent writes need a rule or must both survive.
- **Core:** An LWW register ordered by a timestamp with replica id tie-break.
- **Follow-up 1:** A multi-value register that keeps concurrent writes. *Test:* oracle from the causal history.
- **Follow-up 2:** Clock skew shows LWW losing a causally later write. *Test:* scripted schedule.
- **Follow-up 3:** Hybrid logical clock stamps fix it. *Test:* same schedule passes.
- **Trade-offs:** Silent loss vs surfacing conflicts. **Concepts:** causality, ties, version vectors.
- **After this:** you can choose conflict behaviour. **In projects:** settings sync.
- **Required first:** `iv-crdt-counters` (CRDT 1). **Helpful:** `iv-lamport-vclock` (DIST 6). **Used again in:** `iv-crdt-map` (CRDT 7).
- **Cookbook:** similar `crdts` (LWW register), related `vector-clocks-hlc`. **Status:** to be written. **Runner:** gradable today.

#### CRDT 3. Sets that converge: G, 2P and observed-remove

`iv-crdt-set` · 60 min · medium · P1 · Tracks: Y13

- **New idea:** unique tags let add and remove commute.
- **Core:** A grow-only set, a two-phase set and an OR-Set with unique tags.
- **Follow-up 1:** Concurrent add and remove resolve as add-wins. *Test:* history oracle.
- **Follow-up 2:** Random merge schedules with duplicates converge. *Test:* seeded delivery orders.
- **Follow-up 3:** Show what the two-phase set cannot do. *Test:* re-add scenario.
- **Trade-offs:** Tombstone growth. **Concepts:** tags, tombstones, merge laws.
- **After this:** you can sync collections. **In projects:** shopping carts and tags.
- **Required first:** `iv-crdt-counters` (CRDT 1). **Used again in:** `iv-crdt-convergence` (CRDT 4), `iv-crdt-causal-context` (CRDT 5), `iv-crdt-opbased` (CRDT 9).
- **Cookbook:** similar\* `crdts` (G-set and OR-set with law tests). **Status:** staged draft: `y13-or-set-crdt`. **Runner:** gradable today.

#### CRDT 4. A convergence harness for replicas

`iv-crdt-convergence` · 90 min · hard · P1 · Tracks: Y5

- **New idea:** test convergence over random delivery, then intent.
- **Core:** Given a Replica trait, generate random operations and delivery orders and check convergence.
- **Follow-up 1:** Loss, duplication, partition and heal, all from one seed. *Test:* same seed gives the same trace.
- **Follow-up 2:** Shrink a failing schedule. *Test:* reported schedule is minimal for planted bugs.
- **Follow-up 3:** Check intent properties, not only equality: add-wins, no lost increment. *Test:* broken replicas are rejected.
- **Trade-offs:** Convergence is not correctness. **Concepts:** simulation, property testing, shrinking.
- **After this:** you can find CRDT bugs. **In projects:** any replicated type.
- **Required first:** `iv-crdt-set` (CRDT 3). **Helpful:** `iv-shrinker` (TEST 3), `iv-sim-network` (DIST 1).
- **Cookbook:** similar `crdts` (law tests), similar `deterministic-simulation-testing`. **Status:** to be written. **Runner:** gradable today.

#### CRDT 5. Dots and causal context

`iv-crdt-causal-context` · 90 min · hard · P2 · Tracks: Y13

- **New idea:** remember which events were seen.
- **Core:** Dots (replica, counter) and a causal context that records which events were seen.
- **Follow-up 1:** Compact into a version vector plus a cloud of gaps. *Test:* equals a HashSet of dots.
- **Follow-up 2:** Join of two contexts. *Test:* set-union oracle.
- **Follow-up 3:** Memory bound after in-order delivery. *Test:* cloud is empty.
- **Trade-offs:** Why gaps appear. **Concepts:** dotted version vectors.
- **After this:** you can replace tombstones with causal metadata. **In projects:** sync engines.
- **Required first:** `iv-crdt-set` (CRDT 3). **Helpful:** `iv-lamport-vclock` (DIST 6). **Used again in:** `iv-crdt-orswot` (CRDT 6), `iv-crdt-gc` (CRDT 10).
- **Cookbook:** related `crdts`, related `vector-clocks-hlc`. **Status:** to be written. **Runner:** gradable today.

#### CRDT 6. An OR-Set without tombstones

`iv-crdt-orswot` · 90 min · hard · P2 · Tracks: Y13

- **New idea:** observed-remove without tombstones.
- **Core:** Add-wins set that uses dots and a causal context instead of tombstones.
- **Follow-up 1:** Remove then merge with a replica that has not seen the add. *Test:* history oracle.
- **Follow-up 2:** State size stays proportional to live elements. *Test:* churn test.
- **Follow-up 3:** Random delivery with duplicates and reordering converges. *Test:* seeded schedules.
- **Trade-offs:** Context vs tombstones. **Concepts:** causal context, add-wins.
- **After this:** you can keep replicated sets small. **In projects:** databases.
- **Required first:** `iv-crdt-causal-context` (CRDT 5). **Used again in:** `iv-crdt-map` (CRDT 7).
- **Cookbook:** related `crdts`. **Status:** to be written. **Runner:** gradable today.

#### CRDT 7. A CRDT map with nested values

`iv-crdt-map` · 90 min · hard · P2 · Tracks: Y13

- **New idea:** composing mergeable values and defining removal.
- **Core:** A map whose values are themselves mergeable, generic over a Merge trait.
- **Follow-up 1:** Remove versus concurrent update: choose and enforce update-wins. *Test:* history oracle.
- **Follow-up 2:** Nesting two levels. *Test:* law tests.
- **Follow-up 3:** Delta of a single key. *Test:* applying it equals merging the whole.
- **Trade-offs:** Reset semantics are the hard part. **Concepts:** composition, traits, observed remove.
- **After this:** you can sync nested documents. **In projects:** app state.
- **Required first:** `iv-crdt-orswot` (CRDT 6). **Helpful:** `iv-crdt-registers` (CRDT 2). **Used again in:** `iv-crdt-delta` (CRDT 8).
- **Cookbook:** similar `crdts` (LWW map), similar `crdt-todo-app-no-db` (composed CRDTs). **Status:** to be written. **Runner:** gradable today.

#### CRDT 8. Delta-state replication

`iv-crdt-delta` · 90 min · hard · P2 · Tracks: Y13

- **New idea:** ship only the change, merge it like state.
- **Core:** Each mutation returns a small delta that merges like a state.
- **Follow-up 1:** Delta buffers per neighbour with acknowledgements. *Test:* converges under loss.
- **Follow-up 2:** Fall back to full state after buffer eviction. *Test:* converges.
- **Follow-up 3:** Bytes sent compared with full-state sync. *Test:* bound.
- **Trade-offs:** Causal delta merging. **Concepts:** delta mutators, anti-entropy.
- **After this:** you can cut sync bandwidth. **In projects:** mobile sync.
- **Required first:** `iv-crdt-map` (CRDT 7). **Helpful:** `iv-dist-delivery` (DIST 2). **Used again in:** `iv-crdt-gc` (CRDT 10).
- **Cookbook:** related `crdt-todo-app-no-db`. **Status:** to be written. **Runner:** gradable today.

#### CRDT 9. Operation-based CRDTs and causal broadcast

`iv-crdt-opbased` · 90 min · hard · P2 · Tracks: Y13

- **New idea:** operations need causal delivery.
- **Core:** Prepare and effect phases for a counter and an OR-Set.
- **Follow-up 1:** A causal delivery layer with a holdback queue. *Test:* random reorderings.
- **Follow-up 2:** Exactly-once effect under duplicates. *Test:* duplicated delivery.
- **Follow-up 3:** Show divergence when causality is violated. *Test:* scripted.
- **Trade-offs:** What the network must guarantee. **Concepts:** causal broadcast, commutativity.
- **After this:** you can compare op-based and state-based. **In projects:** collaboration servers.
- **Required first:** `iv-crdt-set` (CRDT 3). **Helpful:** `iv-lamport-vclock` (DIST 6), `iv-dist-delivery` (DIST 2). **Used again in:** `iv-text-crdt` (CRDT 11), `iv-crdt-movable-tree` (CRDT 14).
- **Cookbook:** similar `vector-clocks-hlc` (causal delivery). **Status:** to be written. **Runner:** gradable today.

#### CRDT 10. Collecting tombstones safely

`iv-crdt-gc` · 90 min · hard · P3 · Tracks: Y13

- **New idea:** delete metadata only when everyone has seen it.
- **Core:** Compute the causally stable frontier from what every replica has acknowledged.
- **Follow-up 1:** Drop tombstones below it. *Test:* converges, size bound.
- **Follow-up 2:** A slow or removed replica. *Test:* never resurrects a deleted element.
- **Follow-up 3:** Epochs for membership change. *Test:* scripted.
- **Trade-offs:** Liveness vs safety of collection. **Concepts:** causal stability.
- **After this:** you can bound memory. **In projects:** long-lived documents.
- **Required first:** `iv-crdt-delta` (CRDT 8). **Helpful:** `iv-crdt-causal-context` (CRDT 5).
- **Cookbook:** related `crdt-todo-app-no-db`. **Status:** to be written. **Runner:** gradable today.

#### CRDT 11. Sequence CRDT for text

`iv-text-crdt` · 120 min · hard · P3 · Tracks: Y13

- **New idea:** every character has a stable position id.
- **Core:** RGA insert and delete with unique ids.
- **Follow-up 1:** Causal buffering. *Test:* random schedules converge.
- **Follow-up 2:** No interleaving of concurrent words. *Test:* scenario.
- **Follow-up 3:** Tombstone compaction. *Test:* convergence kept.
- **Trade-offs:** RGA vs Fugue. **Concepts:** sequence CRDT.
- **After this:** you can build collaborative text. **In projects:** editors.
- **Required first:** `iv-crdt-opbased` (CRDT 9). **Used again in:** `iv-crdt-text-fugue` (CRDT 12), `iv-crdt-undo` (CRDT 13), `iv-ot` (CRDT 15).
- **Cookbook:** similar\* `crdt-fugue-text-editor`, similar `crdts`. **Status:** to be written. **Runner:** gradable today.

#### CRDT 12. Concurrent text without interleaving

`iv-crdt-text-fugue` · 120 min · hard · P3 · Tracks: Y13

- **New idea:** tree ordering stops interleaving.
- **Core:** Tree-based sequence CRDT (insert as left or right child).
- **Follow-up 1:** Two users typing words at one point stay unbroken. *Test:* scenario the simpler design fails.
- **Follow-up 2:** Random concurrent edits converge. *Test:* seeded schedules.
- **Follow-up 3:** Run-length encoding of operations. *Test:* round trip, size bound.
- **Trade-offs:** Interleaving anomalies. **Concepts:** sequence CRDT, tree order.
- **After this:** you can choose a text algorithm. **In projects:** editors.
- **Required first:** `iv-text-crdt` (CRDT 11). **Helpful:** `iv-treap-order-stats` (COLL 15).
- **Cookbook:** similar\* `crdt-fugue-text-editor`. **Status:** to be written. **Runner:** gradable today.

#### CRDT 13. Undo in a collaborative document

`iv-crdt-undo` · 90 min · hard · P3 · Tracks: Y13

- **New idea:** undo your own action amid others.
- **Core:** Undo only your own last operation on a replicated list.
- **Follow-up 1:** Redo. *Test:* scripts.
- **Follow-up 2:** Undo after remote edits to the same region. *Test:* converges.
- **Follow-up 3:** Undo a delete that others also deleted. *Test:* defined rule.
- **Trade-offs:** What users expect. **Concepts:** selective undo.
- **After this:** you can add collaborative undo. **In projects:** editors.
- **Required first:** `iv-text-crdt` (CRDT 11). **Helpful:** `iv-undo-stack` (OWN 4).
- **Cookbook:** similar `crdt-block-editor-notion-like`, related `undo-redo-editors`. **Status:** to be written. **Runner:** gradable today.

#### CRDT 14. Moving nodes in a replicated tree

`iv-crdt-movable-tree` · 120 min · hard · P3 · Tracks: Y13

- **New idea:** order moves by time and redo them to avoid cycles.
- **Core:** Concurrent move operations on a tree without creating cycles.
- **Follow-up 1:** Undo and redo ops to apply in timestamp order. *Test:* all orders give one tree.
- **Follow-up 2:** No cycle and no duplication. *Test:* invariant after random schedules.
- **Follow-up 3:** Delete as move to trash. *Test:* scripted.
- **Trade-offs:** Why naive moves break. **Concepts:** timestamps, undo-redo log.
- **After this:** you can sync outlines and folders. **In projects:** block editors.
- **Required first:** `iv-crdt-opbased` (CRDT 9). **Helpful:** `iv-file-system-tree` (PAT 11).
- **Cookbook:** similar\* `crdt-block-editor-notion-like` (its movable tree). **Status:** to be written. **Runner:** gradable today.

#### CRDT 15. Operational transformation

`iv-ot` · 120 min · hard · P3 · Tracks: Y9

- **New idea:** transform operations instead of giving them ids.
- **Core:** transform for insert and delete.
- **Follow-up 1:** Server with revision history. *Test:* random clients converge.
- **Follow-up 2:** Client buffering of pending ops. *Test:* converge.
- **Follow-up 3:** Ranges instead of single chars. *Test:* property.
- **Trade-offs:** OT vs CRDT. **Concepts:** transformation.
- **After this:** you can compare OT with CRDTs. **In projects:** editors.
- **Required first:** `iv-text-crdt` (CRDT 11).
- **Cookbook:** similar\* `operational-transform`. **Status:** exists: `y9-ot-collaborative-text`. **Runner:** gradable today.

### LIN: Data lineage and event history

State as facts, and where every value came from. *Core series, 10 parts, about 12 h. Ends in `cap-lineage`.* Alongside it: `iv-test-differential` (TEST 6).

#### LIN 1. Line diff

`iv-diff` · 60 min · medium · P2 · Tracks: D

- **New idea:** the shortest edit between two versions.
- **Core:** LCS-based diff with operations.
- **Follow-up 1:** Myers shortest edit script. *Test:* length equals DP oracle.
- **Follow-up 2:** Hunks with context. *Test:* golden.
- **Follow-up 3:** Three-way merge with conflicts. *Test:* scenarios.
- **Trade-offs:** Memory. **Concepts:** LCS, Myers.
- **After this:** you can show what changed. **In projects:** review tools.
- **Required first:** `iv-alg-dp-strings` (DP 4). **Used again in:** `iv-git-objects` (LIN 3).
- **Cookbook:** similar `dp-lcs-diff`. **Status:** exists: `y8-shortest-edit-script`. **Runner:** gradable today.

#### LIN 2. Merkle tree with proofs

`iv-merkle` · 60 min · medium · P2 · Tracks: D

- **New idea:** a hash of hashes proves content.
- **Core:** Root and inclusion proofs.
- **Follow-up 1:** Verify rejects tampering. *Test:* mutations.
- **Follow-up 2:** Append-only incremental root. *Test:* equals rebuild.
- **Follow-up 3:** Diff two trees to find changed keys. *Test:* comparison counter.
- **Trade-offs:** Domain separation. **Concepts:** hashing trees.
- **After this:** you can verify data. **In projects:** sync and ledgers.
- **Required first:** nothing. **Used again in:** `iv-dist-anti-entropy` (DIST 11), `iv-git-objects` (LIN 3).
- **Cookbook:** similar\* `merkle-tree`. **Status:** to be written. **Runner:** ok (sha2).

#### LIN 3. Content-addressed object store

`iv-git-objects` · 120 min · medium · P2 · Tracks: D

- **New idea:** content addresses give history for free.
- **Core:** Store blobs by hash; trees and commits.
- **Follow-up 1:** Snapshot a nested map and check out. *Test:* round trip.
- **Follow-up 2:** Diff two commits. *Test:* oracle.
- **Follow-up 3:** Log and merge base on the commit DAG. *Test:* brute force.
- **Trade-offs:** Deduplication. **Concepts:** content addressing, DAG.
- **After this:** you can explain git. **In projects:** artifact stores.
- **Required first:** `iv-merkle` (LIN 2). **Helpful:** `iv-graph-ownership` (OWN 6), `iv-diff` (LIN 1).
- **Cookbook:** similar `merkle-tree`, related `dp-lcs-diff`. **Status:** to be written. **Runner:** ok (sha2; in-memory).

#### LIN 4. An append-only event store

`iv-lin-event-store` · 60 min · medium · P1 · Tracks: Y13

- **New idea:** append facts with an expected version.
- **Core:** Streams with expected-version appends and global order.
- **Follow-up 1:** Conflicts under interleaved writers. *Test:* seeded interleavings.
- **Follow-up 2:** Atomic multi-event batches. *Test:* partial batch invisible.
- **Follow-up 3:** Replay into an aggregate. *Test:* model.
- **Trade-offs:** Optimistic concurrency. **Concepts:** event sourcing, versioning.
- **After this:** you can record history safely. **In projects:** ledgers and audit.
- **Required first:** `iv-wal` (STORE 2). **Helpful:** `iv-occ` (TXN 5). **Used again in:** `iv-lin-projections` (LIN 5), `iv-lin-upcasting` (LIN 6).
- **Cookbook:** similar `event-sourced-ledger`. **Status:** to be written. **Runner:** gradable today.

#### LIN 5. Rebuildable read models

`iv-lin-projections` · 60 min · medium · P2 · Tracks: Y13

- **New idea:** derived views follow the log by checkpoint.
- **Core:** Projections that follow the log with checkpoints.
- **Follow-up 1:** Rebuild equals incremental. *Test:* property.
- **Follow-up 2:** Crash between apply and checkpoint. *Test:* no double count.
- **Follow-up 3:** Snapshots. *Test:* snapshot plus tail equals full replay.
- **Trade-offs:** At-least-once projection. **Concepts:** checkpoints, idempotency.
- **After this:** you can build read models. **In projects:** CQRS services.
- **Required first:** `iv-lin-event-store` (LIN 4). **Helpful:** `iv-idempotency-store` (RESIL 4).
- **Cookbook:** similar `event-sourced-ledger`, related `incremental-view-maintenance-dbsp`. **Status:** to be written. **Runner:** gradable today.

#### LIN 6. Evolve event schemas

`iv-lin-upcasting` · 45 min · medium · P3 · Tracks: F7

- **New idea:** old events are translated on read.
- **Core:** Upcast old event versions on read.
- **Follow-up 1:** A chain of upcasters. *Test:* golden old payloads.
- **Follow-up 2:** Unknown future events. *Test:* policy.
- **Follow-up 3:** Compensating events instead of edits. *Test:* state oracle.
- **Trade-offs:** Never rewrite history. **Concepts:** schema evolution.
- **After this:** you can evolve schemas. **In projects:** long-lived logs.
- **Required first:** `iv-lin-event-store` (LIN 4). **Helpful:** `iv-serde-wire` (BIN 3).
- **Cookbook:** similar `event-sourced-ledger` (upcasters). **Status:** to be written. **Runner:** ok (serde).

#### LIN 7. Dataset lineage graph

`iv-lin-graph` · 60 min · medium · P1 · Tracks: DSA

- **New idea:** derivations form a graph over versions.
- **Core:** Record which runs read and wrote which dataset versions; query upstream and downstream.
- **Follow-up 1:** Impact of a bad input: everything derived from it. *Test:* reachability oracle.
- **Follow-up 2:** As-of queries at an earlier time. *Test:* replay oracle.
- **Follow-up 3:** Reject records that would create a cycle across versions. *Test:* planted.
- **Trade-offs:** Versioned nodes keep the graph acyclic. **Concepts:** provenance graph, versions.
- **After this:** you can answer where data came from. **In projects:** data platforms.
- **Required first:** `iv-graph-ownership` (OWN 6). **Helpful:** `iv-alg-reach` (GRAPH 2). **Used again in:** `iv-lin-recompute` (LIN 8), `iv-lin-column` (LIN 9).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** ok; no cookbook page is about lineage.

#### LIN 8. Recompute only what is stale

`iv-lin-recompute` · 90 min · hard · P2 · Tracks: DSA

- **New idea:** rebuild only what changed, stopping early.
- **Core:** Given changed inputs and a derivation graph, rebuild the minimum set in a valid order.
- **Follow-up 1:** Early cut-off when an output is unchanged. *Test:* build-everything oracle, rebuild counter.
- **Follow-up 2:** Dynamic dependencies discovered while building. *Test:* oracle.
- **Follow-up 3:** Persist and reload the trace. *Test:* equality.
- **Trade-offs:** Make-style vs salsa-style. **Concepts:** incremental computation, content hashes.
- **After this:** you can incremental pipelines. **In projects:** build and data tools.
- **Required first:** `iv-lin-graph` (LIN 7). **Helpful:** `iv-alg-order` (GRAPH 4), `iv-memoize` (ITER 3).
- <details><summary>Links hidden until asked</summary>used again in: `iv-alg-co-dag-dp` (COMBO 4)</details>
- **Cookbook:** similar `incremental-view-maintenance-dbsp`. **Status:** to be written. **Runner:** gradable today.

#### LIN 9. Column-level lineage through a query plan

`iv-lin-column` · 90 min · hard · P2 · Tracks: N

- **New idea:** trace each output column to its inputs.
- **Core:** For a small plan of project, filter, join and aggregate, compute which input columns feed each output column.
- **Follow-up 1:** Distinguish value flow from filter influence. *Test:* golden plans.
- **Follow-up 2:** Check against perturbation: changing an unrelated input never changes the output. *Test:* random data.
- **Follow-up 3:** Through several views. *Test:* composition property.
- **Trade-offs:** Static lineage over-approximates. **Concepts:** tree walking, set propagation.
- **After this:** you can column-level lineage. **In projects:** governance tools.
- **Required first:** `iv-lin-graph` (LIN 7). **Helpful:** `iv-volcano` (QUERY 2), `iv-planner-rules` (QUERY 9).
- **Cookbook:** related `relational-algebra-engine`. **Status:** to be written. **Runner:** ok; no cookbook page is about lineage.

#### LIN 10. Aggregates as decide and evolve

`iv-decide-evolve` · 90 min · hard · P2 · prep · Tracks: LIN

- **New idea:** state is a fold of facts, and rules are a pure function that proposes new facts.
- **Core:** Model a small aggregate with commands, events and errors, a pure decide and a total evolve, tested as given, when, then.
- **Follow-up 1:** snapshot plus tail. *Test:* equals full fold for every split point ; seeded command sequences :: invariants hold and replay is deterministic.
- **Required first:** `iv-vending-machine` (PAT 1).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

### MON: Monitoring and debugging

See what a system is doing. Take parts alongside other series. *Core series, 9 parts, about 8 h. Ends in `cap-loadbalancer`.*

#### MON 1. Statistics counters

`iv-atomic-counters` · 45 min · medium · P2 · Tracks: C3

- **New idea:** record from many threads without a lock.
- **Core:** record(v) from many threads, snapshot count, sum, min, max.
- **Follow-up 1:** Sharded counter with padding. *Test:* exact totals.
- **Follow-up 2:** Log-linear latency histogram with merge. *Test:* quantile error bound.
- **Follow-up 3:** Sloppy counter with a stated bound. *Test:* bound checked.
- **Trade-offs:** Relaxed ordering; false sharing. **Concepts:** atomics, fetch_update, histograms.
- **After this:** you can count hot paths. **In projects:** servers.
- **Required first:** `iv-spinlock` (ATOM 1). **Used again in:** `iv-snowflake` (RESIL 9), `iv-mon-metrics` (MON 2), `iv-log-linear-histogram` (MON 9).
- **Cookbook:** similar\* `concurrent-counters-and-statistics`. **Status:** exists: `c3-atomic-stats`. **Runner:** gradable today.

#### MON 2. A metrics registry with text exposition

`iv-mon-metrics` · 60 min · medium · P1 · Tracks: B6

- **New idea:** labelled counters and histograms with limits.
- **Core:** Counters, gauges and labelled families rendered as text.
- **Follow-up 1:** Histograms with cumulative buckets. *Test:* golden output.
- **Follow-up 2:** Label cardinality limit. *Test:* overflow bucket.
- **Follow-up 3:** Concurrent updates. *Test:* exact totals.
- **Trade-offs:** Cardinality cost. **Concepts:** metrics, atomics, labels.
- **After this:** you can expose metrics. **In projects:** services.
- **Required first:** nothing. **Helpful:** `iv-atomic-counters` (MON 1).
- **Cookbook:** similar\* `metrics-prometheus-histograms-load-testing`. **Status:** to be written (partly practised by `c3-atomic-stats`). **Runner:** gradable today.

#### MON 3. A flight recorder for post-mortems

`iv-mon-flight-recorder` · 45 min · medium · P2 · Tracks: C3

- **New idea:** keep the last events for a crash report.
- **Core:** A fixed ring of recent events that overwrites the oldest.
- **Follow-up 1:** Dump on panic in order. *Test:* panic test.
- **Follow-up 2:** Many writers. *Test:* no torn entries.
- **Follow-up 3:** Zero allocation when recording. *Test:* counting.
- **Trade-offs:** What to keep. **Concepts:** overwrite ring, panic hooks.
- **After this:** you can debug rare failures. **In projects:** servers.
- **Required first:** `iv-ring-deque` (COLL 2).
- **Cookbook:** similar `ring-buffer-zoo` (flight recorder). **Status:** to be written. **Runner:** gradable today.

#### MON 4. Request tracing with span trees

`iv-mon-spans` · 60 min · medium · P2 · Tracks: B6

- **New idea:** causally linked timed spans.
- **Core:** Spans with parents, entered and exited by guards, rendered as a tree.
- **Follow-up 1:** Propagate context in a W3C-style header. *Test:* parse and reject corpus.
- **Follow-up 2:** Critical path of a trace. *Test:* brute force.
- **Follow-up 3:** Sampling that keeps whole traces. *Test:* property.
- **Trade-offs:** Context across threads. **Concepts:** RAII guards, trees.
- **After this:** you can trace requests. **In projects:** services.
- **Required first:** `iv-raii-pool` (SMART 1).
- **Cookbook:** similar\* `clock-sync-and-trace-context` (trace context and critical path). **Status:** to be written (partly practised by `l1-raii-span-guard`). **Runner:** gradable today.

#### MON 5. Alert on error budget burn

`iv-mon-slo` · 60 min · medium · P3 · Tracks: B6

- **New idea:** alert on how fast the error budget burns.
- **Core:** Sliding error ratios over a fake clock.
- **Follow-up 1:** Multi-window burn-rate alerts. *Test:* scripted incidents.
- **Follow-up 2:** Flap suppression. *Test:* scripts.
- **Follow-up 3:** Bounded memory. *Test:* size check.
- **Trade-offs:** Detection time vs noise. **Concepts:** sliding windows.
- **After this:** you can page on what matters. **In projects:** operations.
- **Required first:** `iv-hit-counter` (CACHE 3).
- **Cookbook:** related `metrics-prometheus-histograms-load-testing`. **Status:** to be written. **Runner:** gradable today.

#### MON 6. Find lock-order bugs at run time

`iv-mon-lockdep` · 60 min · hard · P2 · Tracks: C1

- **New idea:** a cycle in lock order predicts deadlock.
- **Core:** Wrap locks, record acquisition order and report a cycle before it deadlocks.
- **Follow-up 1:** Report both stacks of names. *Test:* golden.
- **Follow-up 2:** Reader-writer locks. *Test:* scripts.
- **Follow-up 3:** Low overhead path when disabled. *Test:* counter.
- **Trade-offs:** What it can and cannot find. **Concepts:** graph cycles, thread-local state.
- **After this:** you can catch deadlocks in tests. **In projects:** concurrent services.
- **Required first:** `iv-bank-transfer-concurrent` (CONC 3).
- <details><summary>Links hidden until asked</summary>helpful: `iv-alg-order` (GRAPH 4)</details>
- **Cookbook:** similar\* `concurrency-interview-questions` (its Q11). **Status:** to be written. **Runner:** gradable today.

#### MON 7. Fold stack samples into a flame graph

`iv-mon-flame` · 60 min · medium · P3 · Tracks: F1

- **New idea:** aggregate stacks to find where time goes.
- **Core:** Aggregate sampled stacks into a call tree with self and total time.
- **Follow-up 1:** Folded-stack text round trip. *Test:* property.
- **Follow-up 2:** Diff two profiles. *Test:* oracle.
- **Follow-up 3:** Find the top hot paths with recursion handled. *Test:* brute force.
- **Trade-offs:** Sampling bias. **Concepts:** trees, aggregation.
- **After this:** you can read profiles. **In projects:** performance work.
- **Required first:** `iv-trie` (COLL 6).
- **Cookbook:** related `perf-checklist`. **Status:** to be written. **Runner:** gradable today.

#### MON 8. Detect anomalies in a latency series

`iv-mon-anomaly` · 60 min · medium · P3 · Tracks: B6

- **New idea:** compare each value with a robust baseline.
- **Core:** EWMA mean and deviation with alerts.
- **Follow-up 1:** Robust z-score over a window. *Test:* sorting oracle.
- **Follow-up 2:** Quantile sketch with merge. *Test:* error bound.
- **Follow-up 3:** Measured false-alarm rate on seeded noise. *Test:* bound.
- **Trade-offs:** Sensitivity. **Concepts:** streaming statistics.
- **After this:** you can detect regressions. **In projects:** operations.
- **Required first:** `iv-median-stream` (COLL 5).
- **Cookbook:** similar\* `timeseries-sketches-downsampling-anomaly`. **Status:** to be written. **Runner:** gradable today.

#### MON 9. A fixed-memory latency histogram

`iv-log-linear-histogram` · 60 min · medium · P2 · prep · Tracks: MON

- **New idea:** buckets spaced by power of two with linear sub-buckets bound the relative error.
- **Core:** Implement index, upper bound, concurrent record and quantile with at most 12.5 percent relative error.
- **Follow-up 1:** cumulative less-or-equal counts. *Test:* compare with a sorted vector model ; merge two histograms :: equals recording all values in one.
- **Required first:** `iv-alg-bit-tricks` (BIT 3), `iv-atomic-counters` (MON 1).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

### TEST: Testing and reliability

Make failures reproducible, then hunt them. Take parts alongside other series. *Core series, 12 parts, about 12 h. Ends in `cap-lease`.*

#### TEST 1. Make a time-dependent service testable

`iv-fake-clock` · 45 min · easy · P1 · Tracks: Y5

- **New idea:** inject time and other side effects.
- **Core:** Introduce Clock and Sleeper traits into a service.
- **Follow-up 1:** Deterministic timeout and retry tests. *Test:* no sleeps.
- **Follow-up 2:** Fake transport with fault script. *Test:* scripted.
- **Follow-up 3:** tokio paused time version. *Test:* same.
- **Trade-offs:** Where to inject. **Concepts:** dependency injection, traits.
- **After this:** you can test timeouts without sleeping. **In projects:** every service.
- **Required first:** nothing. **Used again in:** `iv-effect-log` (STORE 17), `iv-retry-backoff` (RESIL 1), `iv-rate-limiter` (RESIL 2), `iv-window-limiters` (RESIL 10), `iv-sim-network` (DIST 1), `iv-bench-harness` (PERF 1).
- **Cookbook:** similar `testing-patterns`, similar `std-time-instant-duration-systemtime`. **Status:** to be written. **Runner:** gradable today.

#### TEST 2. Write tests that kill planted bugs

`iv-write-tests` · 60 min · medium · P1 · Tracks: Y5

- **New idea:** a model plus random operations finds bugs.
- **Core:** Given an interface and several broken implementations, write the test that rejects each.
- **Follow-up 1:** Seeded random model test. *Test:* every mutant caught, correct one passes.
- **Follow-up 2:** Failure message with seed and trace. *Test:* reviewed.
- **Follow-up 3:** Edge enumeration. *Test:* more mutants.
- **Trade-offs:** Oracles and invariants. **Concepts:** model-based testing.
- **After this:** you can test data structures. **In projects:** any library.
- **Required first:** nothing. **Used again in:** `iv-shrinker` (TEST 3), `iv-fuzz-harness` (TEST 5), `iv-test-differential` (TEST 6), `iv-test-failpoints` (TEST 7).
- **Cookbook:** similar `testing-patterns`, similar `lru-cache` (model test). **Status:** staged draft: `y5-catch-the-broken-queue`. **Runner:** ok (the staged queue problem shows the grading pattern).

#### TEST 3. Shrinker and a tiny property framework

`iv-shrinker` · 60 min · medium · P1 · Tracks: Y5

- **New idea:** reduce a failing input to the cause.
- **Core:** Greedy shrink of a failing vector.
- **Follow-up 1:** Generators as a trait with composition. *Test:* determinism.
- **Follow-up 2:** Call budget. *Test:* bound.
- **Follow-up 3:** Shrink structured values. *Test:* minimality on known bugs.
- **Trade-offs:** quickcheck vs proptest. **Concepts:** shrinking, trait design.
- **After this:** you can debug random failures. **In projects:** test tooling.
- **Required first:** `iv-write-tests` (TEST 2). **Used again in:** `iv-crdt-convergence` (CRDT 4), `iv-proptest-use` (TEST 4), `iv-test-differential` (TEST 6).
- **Cookbook:** similar\* `shrinking` (greedy loop). **Status:** staged draft: `y5-shrink-failing-input`. **Runner:** gradable today.

#### TEST 4. Property tests with proptest

`iv-proptest-use` · 60 min · medium · P2 · Tracks: Y5

- **New idea:** generate from properties with a library.
- **Core:** Write properties for a codec.
- **Follow-up 1:** Custom strategies. *Test:* mutants caught.
- **Follow-up 2:** State-machine style ops. *Test:* mutants caught.
- **Follow-up 3:** Regression seeds. *Test:* reviewed.
- **Trade-offs:** Property choice. **Concepts:** proptest.
- **After this:** you can property-test codecs. **In projects:** Rust projects.
- **Required first:** `iv-shrinker` (TEST 3).
- **Cookbook:** similar `criterion-proptest-insta-testing`. **Status:** to be written. **Runner:** ok (proptest vendored); grading a learner's properties needs mutant builds like the queue problem.

#### TEST 5. Make a parser survive fuzzing

`iv-fuzz-harness` · 60 min · medium · P2 · Tracks: Y5

- **New idea:** treat every input as hostile.
- **Core:** Fix a parser so no input panics.
- **Follow-up 1:** Differential round trip. *Test:* random and mutated inputs.
- **Follow-up 2:** Allocation limits from declared lengths. *Test:* counting.
- **Follow-up 3:** Depth limits. *Test:* deep input.
- **Trade-offs:** Coverage-guided vs random. **Concepts:** fuzzing, robustness.
- **After this:** you can harden parsers. **In projects:** network code.
- **Required first:** `iv-write-tests` (TEST 2). **Used again in:** `iv-json` (PARSE 13).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** random-input tests only; cargo-fuzz needs nightly and is not available.

#### TEST 6. Differential testing of two engines

`iv-test-differential` · 60 min · medium · P1 · Tracks: Y5

- **New idea:** two implementations are each other's oracle.
- **Core:** Generate random queries and compare a clever engine with a naive one.
- **Follow-up 1:** Metamorphic relations where there is no oracle. *Test:* planted bugs.
- **Follow-up 2:** Reduce a failing case. *Test:* size bound.
- **Follow-up 3:** Deterministic reporting by seed. *Test:* reviewed.
- **Trade-offs:** Oracle independence. **Concepts:** differential and metamorphic testing.
- **After this:** you can test optimisers. **In projects:** databases and compilers.
- **Required first:** `iv-write-tests` (TEST 2). **Helpful:** `iv-shrinker` (TEST 3).
- **Cookbook:** similar\* `consistent-snapshots-sql-logic-tests`. **Status:** to be written. **Runner:** gradable today.

#### TEST 7. Failpoints for crash testing

`iv-test-failpoints` · 60 min · medium · P2 · Tracks: Y5

- **New idea:** crash at every chosen point.
- **Core:** A registry of named failpoints compiled into a storage routine through a trait.
- **Follow-up 1:** Enumerate every crash point and check recovery. *Test:* planted ordering bug found.
- **Follow-up 2:** Torn and reordered writes in a fake disk. *Test:* invariant.
- **Follow-up 3:** Nth-hit triggers. *Test:* scripts.
- **Trade-offs:** Fault models. **Concepts:** fault injection.
- **After this:** you can test recovery. **In projects:** storage systems.
- **Required first:** `iv-write-tests` (TEST 2). **Helpful:** `iv-wal` (STORE 2).
- **Cookbook:** similar `toy-filesystem` (crash at every write), similar `logging-schemes-group-commit`. **Status:** to be written. **Runner:** gradable today.

#### TEST 8. Seeded xorshift and shuffling

`iv-seeded-rng` · 45 min · medium · P2 · prep · Tracks: TEST

- **New idea:** a small seeded generator makes every random test replayable.
- **Core:** Implement xorshift64, a bounded draw, a percentage chance and Fisher-Yates shuffle.
- **Follow-up 1:** remove modulo bias by rejection. *Test:* bucket counts within fixed bounds for a fixed seed ; derive independent streams from one seed :: same seed replays the same sequences.
- **Required first:** nothing. **Used again in:** `iv-grammar-program-gen` (TEST 9), `iv-disk-fault-model` (TEST 10), `iv-monte-carlo` (NUM 8).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

#### TEST 9. Generating valid random programs

`iv-grammar-program-gen` · 90 min · hard · P2 · prep · Tracks: TEST

- **New idea:** generate from a scoped grammar so every program is well formed and terminates.
- **Core:** Write a seeded generator for a tiny language that tracks names in scope and keeps loop counters read-only.
- **Follow-up 1:** add functions with matching arity. *Test:* every program passes a supplied checker over many seeds ; bias toward rare error paths such as division by zero :: counts of each outcome are non-zero for fixed seeds.
- **Required first:** `iv-seeded-rng` (TEST 8).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

#### TEST 10. A disk that crashes

`iv-disk-fault-model` · 60 min · medium · P2 · prep · Tracks: TEST

- **New idea:** writes not yet synced may be lost, reordered or torn at a crash.
- **Core:** Build a wrapper device with a volatile write list, a crash after k writes and seeded post-crash images.
- **Follow-up 1:** show a two-write update that needs a barrier to be safe. *Test:* some seed breaks it without sync and none with sync ; torn writes by 512-byte sector :: every sector is wholly old or new.
- **Required first:** `iv-positional-block-io` (IO 8), `iv-seeded-rng` (TEST 8). **Used again in:** `iv-redo-journal` (STORE 16).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

#### TEST 11. Seeded data generators with skew and typos

`iv-seeded-data-gen` · 60 min · medium · P2 · prep · Tracks: TEST

- **New idea:** a small seeded generator makes realistic, skewed workloads that replay exactly.
- **Core:** Write a xorshift RNG with below, range, unit float and chance, then generate rows with uniform, Zipf, correlated and nullable columns.
- **Follow-up 1:** Fisher-Yates shuffle and weighted choice. *Test:* frequency bounds on fixed seeds and permutation checks ; String generator and one-edit typo mutator over Unicode :: same seed gives same output and each mutant is within one edit with transposition.
- **Required first:** nothing.
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

#### TEST 12. Hunting bad schedules in a simulation

`iv-sim-invariant-search` · 60 min · medium · P2 · prep · Tracks: TEST

- **New idea:** state safety as a check over history, search seeds for a violation and replay it against the fix.
- **Core:** Given a seeded simulation with a race, write the history invariant, find the first failing seed and replay it with and without the fix using the same random draws.
- **Follow-up 1:** follow-up 1 shrink the failing configuration. *Test:* smaller config still fails and is identical across runs ; follow-up 2 keep schedules identical when the fix changes replies :: event traces compare equal up to the verdicts.
- **Required first:** `iv-sim-network` (DIST 1).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

### PERF: Performance

Measure first, then change one thing. *Core series, 4 parts, about 4 h. Ends in `cap-raytracer`.* Alongside it: `iv-mon-flame` (MON 7).

#### PERF 1. Micro-benchmark harness

`iv-bench-harness` · 60 min · medium · P3 · Tracks: F1

- **New idea:** measure with warm-up, samples and medians.
- **Core:** Warm-up, samples, median with black_box.
- **Follow-up 1:** Outlier handling. *Test:* synthetic timer injected.
- **Follow-up 2:** Compare two functions with a confidence rule. *Test:* synthetic data.
- **Follow-up 3:** Allocation counts. *Test:* counting allocator.
- **Trade-offs:** Pitfalls. **Concepts:** benchmarking.
- **After this:** you can compare changes honestly. **In projects:** performance work.
- **Required first:** `iv-fake-clock` (TEST 1). **Used again in:** `iv-cache-friendly` (PERF 3), `iv-simd-scan` (PERF 4).
- **Cookbook:** similar\* `bench-harness`. **Status:** to be written. **Runner:** criterion cannot grade: only [perf] helpers.

#### PERF 2. Remove allocations from a hot path

`iv-alloc-budget` · 60 min · medium · P1 · Tracks: F1

- **New idea:** count allocations and remove them.
- **Core:** Rewrite a formatter to meet an allocation budget.
- **Follow-up 1:** with_capacity and reuse. *Test:* counts.
- **Follow-up 2:** Cow and borrowing. *Test:* counts.
- **Follow-up 3:** Interning. *Test:* counts.
- **Trade-offs:** Measure first. **Concepts:** allocation, Cow.
- **After this:** you can speed hot paths. **In projects:** servers.
- **Required first:** nothing.
- **Cookbook:** similar `counting-allocator`, related `perf-checklist`. **Status:** to be written (partly practised by `f7-itoa-stack-buffer`, `l5-inner-fn-monomorphization`). **Runner:** ok ([perf] count_allocs).

#### PERF 3. Make a kernel cache friendly

`iv-cache-friendly` · 60 min · medium · P2 · Tracks: F1

- **New idea:** memory access order dominates speed.
- **Core:** Fix traversal order in a matrix kernel.
- **Follow-up 1:** Blocking. *Test:* relative speed gate.
- **Follow-up 2:** Struct of arrays. *Test:* gate.
- **Follow-up 3:** Explain with numbers. *Test:* discussion.
- **Trade-offs:** Benchmark honesty. **Concepts:** locality.
- **After this:** you can restructure data. **In projects:** analytics and games.
- **Required first:** `iv-bench-harness` (PERF 1).
- **Cookbook:** similar `cache-locality`, similar `matmul-optimization`. **Status:** to be written (partly practised by `f2-particles-as-arrays`). **Runner:** [perf] release timing can flake on a busy host.

#### PERF 4. Byte search with SWAR and SIMD

`iv-simd-scan` · 90 min · hard · P3 · Tracks: F5

- **New idea:** process many bytes per step.
- **Core:** memchr with eight bytes per step.
- **Follow-up 1:** count and rfind. *Test:* differential on all alignments.
- **Follow-up 2:** std. *Test:* arch with runtime detection and fallback :: differential.
- **Follow-up 3:** UTF-8 ASCII fast path. *Test:* differential.
- **Trade-offs:** When the compiler already vectorises. **Concepts:** SWAR, SIMD.
- **After this:** you can speed scanning. **In projects:** parsers.
- **Required first:** `iv-alg-bit-tricks` (BIT 3). **Helpful:** `iv-bench-harness` (PERF 1).
- **Cookbook:** similar\* `simd-intrinsics`. **Status:** to be written (partly practised by `f5-swar-memchr`). **Runner:** ok for SWAR; std::arch results depend on the runner CPU.

### NUM: Numbers and pixels

Optional: exact arithmetic and rasterisation. *Branch series, 9 parts, about 10 h. Ends in `cap-raytracer`.* Alongside it: `iv-test-differential` (TEST 6).

#### NUM 1. Big integer arithmetic

`iv-bigint` · 90 min · medium · P3 · Tracks: S8

- **New idea:** carry and borrow across limbs.
- **Core:** Add, sub, mul on u32 limbs with Display.
- **Follow-up 1:** Parse and operators. *Test:* differential against u128.
- **Follow-up 2:** Karatsuba. *Test:* equals schoolbook.
- **Follow-up 3:** Long division. *Test:* q*b + r identity.
- **Trade-offs:** Limb size. **Concepts:** carries, operator traits.
- **After this:** you can handle huge integers. **In projects:** crypto and finance.
- **Required first:** `iv-generic-matrix` (TRAIT 3).
- **Cookbook:** similar\* `bigint-arithmetic`. **Status:** to be written. **Runner:** gradable today.

#### NUM 2. Software line and triangle drawing

`iv-raster` · 60 min · medium · P3 · Tracks: D

- **New idea:** integer edge rules draw exactly once.
- **Core:** Bresenham lines into a pixel buffer.
- **Follow-up 1:** Triangle fill with a top-left rule. *Test:* no double-covered pixel.
- **Follow-up 2:** Z-buffer. *Test:* order independence.
- **Follow-up 3:** PPM writer. *Test:* golden bytes.
- **Trade-offs:** Integer exactness. **Concepts:** rasterisation.
- **After this:** you can draw primitives. **In projects:** graphics tools.
- **Required first:** nothing.
- **Cookbook:** similar\* `lines-curves-fills`, similar `software-rasterizer`. **Status:** to be written. **Runner:** gradable today.

#### NUM 3. Rays against spheres, planes and triangles

`iv-ray-intersection` · 90 min · hard · P2 · prep · Tracks: NUM

- **New idea:** a hit is the smallest valid parameter t that solves a geometric equation.
- **Core:** Write dot, cross and normalise, then intersect a ray with sphere, plane and triangle within (tmin, tmax) and pick the nearest.
- **Follow-up 1:** surface normals and barycentric coordinates. *Test:* known values and point reconstruction ; axis-aligned box slab test :: agrees with dense point sampling on seeded rays.
- **Required first:** `iv-generic-matrix` (TRAIT 3). **Used again in:** `iv-bvh-flat` (COLL 17), `iv-camera-rays` (NUM 5), `iv-direct-shading` (NUM 6), `iv-surface-optics` (NUM 7), `iv-mesh-subdivision` (NUM 9).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

#### NUM 4. Floating-point comparisons that stay deterministic

`iv-float-robust` · 45 min · medium · P2 · prep · Tracks: NUM

- **New idea:** float code needs explicit rules for ties, tolerances and NaN.
- **Core:** Fix a nearest-item search that gives order-dependent or self-intersecting results, using epsilon offsets, a total tie rule and NaN-safe ordering.
- **Follow-up 1:** keep equal values with next_up. *Test:* permuted inputs give identical winners ; handle infinities and signed zero in reciprocal tests :: table of edge cases.
- **Required first:** nothing.
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

#### NUM 5. Virtual camera: pinhole and thin lens

`iv-camera-rays` · 45 min · medium · P2 · prep · Tracks: NUM

- **New idea:** a camera maps sample coordinates to rays through an orthonormal frame.
- **Core:** Build the frame from position, target and field of view and generate pinhole rays for (s,t).
- **Follow-up 1:** thin lens with disk rejection sampling. *Test:* all lens rays meet on the focus plane ; reject degenerate setups :: view parallel to up returns an error.
- **Required first:** `iv-ray-intersection` (NUM 3).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

#### NUM 6. Local lighting and shadow rays

`iv-direct-shading` · 60 min · medium · P2 · prep · Tracks: NUM

- **New idea:** surface brightness is a sum over visible lights of simple geometric terms.
- **Core:** Compute Lambert shading with inverse-square falloff from point lights, with occlusion tests.
- **Follow-up 1:** Blinn-Phong highlight. *Test:* peak at the mirror half-vector against hand values ; shadow-origin offset :: a lit convex surface has no self-shadow pixels.
- **Required first:** `iv-ray-intersection` (NUM 3).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

#### NUM 7. Reflection, refraction and Fresnel

`iv-surface-optics` · 60 min · medium · P2 · prep · Tracks: NUM

- **New idea:** at a boundary light splits by Snell and Fresnel rules.
- **Core:** Implement reflect, refract with total internal reflection and Schlick reflectance.
- **Follow-up 1:** Beer-Lambert absorption over distance. *Test:* additivity over split segments ; recursive two-surface trace with depth limit :: normal ray through a glass slab keeps direction and energy.
- **Required first:** `iv-ray-intersection` (NUM 3).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

#### NUM 8. Monte Carlo estimators you can test

`iv-monte-carlo` · 90 min · hard · P2 · prep · Tracks: NUM

- **New idea:** a random average estimates an integral, and its error and bias can be checked.
- **Core:** Estimate integrals with uniform and stratified samples and measure RMSE against known answers over fixed seeds.
- **Follow-up 1:** cosine-weighted hemisphere sampling so the pdf cancels. *Test:* mean cosine 2/3 and a constant-light furnace returns the albedo ; Russian roulette on a random-length series :: mean matches the closed form and fails without the weight.
- **Required first:** `iv-seeded-rng` (TEST 8).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

#### NUM 9. Generate meshes by subdivision

`iv-mesh-subdivision` · 45 min · medium · P2 · prep · Tracks: NUM

- **New idea:** shared edges need one canonical key so new vertices are created once.
- **Core:** Subdivide an icosahedron into a sphere with a midpoint cache keyed by the unordered edge.
- **Follow-up 1:** fix outward winding. *Test:* every face normal points away from the centre and every edge is used twice ; boxes rotated about an axis :: corner distances are preserved.
- **Required first:** `iv-ray-intersection` (NUM 3).
- **Cookbook:** no page found for this. **Status:** to be written. **Runner:** gradable today.

### REC: Algorithms: recursion to memoisation

Recursion, backtracking, then remembering answers. *Algorithm series, 6 parts, about 6 h. Ends in `cap-lox`.* Alongside it: `iv-write-tests` (TEST 2).

**DSA overlap:** the DSA area covers Backtracking, Trees with LeetCode problems, Python templates and Learn pages (content/dsa/lessons/*.toml, `/dsa/patterns/...`). Learn the pattern there. These parts are Rust, set in engineering scenarios, with my own statements and brute-force oracles; no LeetCode statement is copied, and nothing in the Learn pages or tags changes.

#### REC 1. Size a nested package tree

`iv-alg-recursion-fs` · 45 min · easy · P2 · Tracks: L1

- **New idea:** a recursive function over a recursive structure.
- **Core:** Recursively total sizes and find the heaviest chain in a nested module tree.
- **Follow-up 1:** Rewrite with an explicit stack and survive depth 1e6. *Test:* scale test.
- **Follow-up 2:** Shared sub-packages counted once. *Test:* BFS oracle.
- **Follow-up 3:** Report the path that forms a loop. *Test:* planted cycles.
- **Trade-offs:** Call stack limits; Box recursion. **Concepts:** recursion, explicit stack.
- **After this:** you can walk any nested data. **In projects:** build and file tools.
- **Required first:** nothing. **Used again in:** `iv-alg-ast-walk` (REC 2), `iv-alg-config-matrix` (REC 3), `iv-alg-regex-count` (REC 5).
- <details><summary>Links hidden until asked</summary>used again in: `iv-alg-co-tree-dp` (COMBO 7)</details>
- **Cookbook:** similar `binary-trees-in-rust`. **Status:** to be written. **Runner:** gradable today.

#### REC 2. Rewrite an expression tree

`iv-alg-ast-walk` · 45 min · medium · P2 · Tracks: L3

- **New idea:** recursion that rewrites owned data in place.
- **Core:** Recursive passes over a boxed enum: evaluate, substitute, simplify in place.
- **Follow-up 1:** Mutate without cloning using mem. *Test:* take :: allocation counting.
- **Follow-up 2:** Index-based arena version of the same tree. *Test:* same results.
- **Follow-up 3:** Post-order without recursion. *Test:* deep input.
- **Trade-offs:** Box vs arena ownership in recursive code. **Concepts:** recursion, ownership, arena.
- **After this:** you can transform trees without clones. **In projects:** compilers.
- **Required first:** `iv-alg-recursion-fs` (REC 1). **Used again in:** `iv-constant-fold` (LANG 2), `iv-predicate-selectivity` (QUERY 14).
- **Cookbook:** related `arena-ids`, related `std-mem-replace-take-swap-forget`. **Status:** to be written. **Runner:** gradable today.

#### REC 3. Enumerate build configurations

`iv-alg-config-matrix` · 60 min · medium · P2 · Tracks: S3

- **New idea:** choose, recurse, undo.
- **Core:** Backtrack over feature choices and yield every valid combination in order.
- **Follow-up 1:** Constraints requires and conflicts with pruning. *Test:* brute force over all subsets, nodes-visited bound.
- **Follow-up 2:** Lazy iterator instead of a Vec. *Test:* take(10) does bounded work.
- **Follow-up 3:** Count without enumerating where independent. *Test:* equals enumeration.
- **Trade-offs:** Pruning vs filtering at the end. **Concepts:** backtracking, pruning.
- **After this:** you can enumerate valid configurations. **In projects:** CI matrices.
- **Required first:** `iv-alg-recursion-fs` (REC 1). **Used again in:** `iv-interleaving-checker` (ATOM 9), `iv-linearizability` (DIST 16), `iv-alg-rota` (REC 4).
- **Cookbook:** similar `backtracking-branch-bound`, related `combinatorial-generation`. **Status:** to be written. **Runner:** gradable today.

#### REC 4. Fill an on-call rota

`iv-alg-rota` · 90 min · medium · P2 · Tracks: S3

- **New idea:** prune and order choices.
- **Core:** Assign people to shifts under availability and rest rules by backtracking.
- **Follow-up 1:** Most-constrained-first ordering. *Test:* node-count bound.
- **Follow-up 2:** Count solutions, handling identical people. *Test:* brute force.
- **Follow-up 3:** Best solution by a fairness score with branch and bound. *Test:* exhaustive oracle.
- **Trade-offs:** Constraint propagation. **Concepts:** backtracking, heuristics, branch and bound.
- **After this:** you can solve constraint puzzles. **In projects:** scheduling.
- **Required first:** `iv-alg-config-matrix` (REC 3). **Used again in:** `iv-alg-memo-resolver` (REC 6).
- <details><summary>Links hidden until asked</summary>used again in: `iv-alg-co-bt-bitset` (COMBO 8)</details>
- **Cookbook:** similar `backtracking-branch-bound`, related `procedural-puzzles`. **Status:** to be written. **Runner:** gradable today.

#### REC 5. From recursion to a table

`iv-alg-regex-count` · 60 min · medium · P3 · Tracks: DP

- **New idea:** overlapping subproblems can be remembered.
- **Core:** Count the ways to split a log line into dictionary fields, recursively.
- **Follow-up 1:** Memoise. *Test:* scale test.
- **Follow-up 2:** Bottom-up with a rolling array. *Test:* same answers on random inputs.
- **Follow-up 3:** Reconstruct one and the lexicographically first split. *Test:* brute force.
- **Trade-offs:** Top-down vs bottom-up. **Concepts:** memoisation, tabulation.
- **After this:** you can speed up recursion. **In projects:** parsing and text tools.
- **Required first:** `iv-alg-recursion-fs` (REC 1). **Helpful:** `iv-memoize` (ITER 3). **Used again in:** `iv-alg-memo-resolver` (REC 6), `iv-alg-dp-1d` (DP 1).
- **Cookbook:** similar `dp-and-backtracking-questions`. **Status:** to be written. **Runner:** gradable today.

#### REC 6. Memoise a dependency resolver

`iv-alg-memo-resolver` · 90 min · medium · P2 · Tracks: S4

- **New idea:** memoise a search over states.
- **Core:** Pick a version for each package so all constraints hold; naive search first.
- **Follow-up 1:** Cache failed states. *Test:* call-count bound on adversarial inputs.
- **Follow-up 2:** Return an explanation of the conflict. *Test:* minimality against brute force.
- **Follow-up 3:** Recursive memo with the borrow checker. *Test:* compiles with a context struct.
- **Trade-offs:** When memoisation changes complexity. **Concepts:** memoisation, backtracking, state keys.
- **After this:** you can resolve dependencies. **In projects:** package managers.
- **Required first:** `iv-alg-rota` (REC 4). **Helpful:** `iv-alg-regex-count` (REC 5).
- **Cookbook:** related `dp-memoization`, similar `sat-solver-cdcl`. **Status:** to be written. **Runner:** gradable today.

### GRAPH: Algorithms: graphs in Rust

Representation first, then traversal, order, paths and structure. *Algorithm series, 7 parts, about 8 h. Ends in `cap-lineage`.* Alongside it: `iv-test-differential` (TEST 6).

**DSA overlap:** the DSA area covers Graphs, Advanced Graphs with LeetCode problems, Python templates and Learn pages (content/dsa/lessons/*.toml, `/dsa/patterns/...`). Learn the pattern there. These parts are Rust, set in engineering scenarios, with my own statements and brute-force oracles; no LeetCode statement is copied, and nothing in the Learn pages or tags changes.

#### GRAPH 1. A graph the borrow checker likes

`iv-alg-graph-repr` · 45 min · medium · P1 · Tracks: L2

- **New idea:** ids and adjacency lists instead of references.
- **Core:** Build a call graph with u32 node ids, adjacency lists and a name interner.
- **Follow-up 1:** CSR form built by counting sort. *Test:* same neighbours as the list form.
- **Follow-up 2:** Remove nodes and keep ids stable. *Test:* model comparison.
- **Follow-up 3:** Annotate nodes while traversing neighbours without clone. *Test:* allocation counting.
- **Trade-offs:** Rc graphs vs index graphs. **Concepts:** index-based graph, newtype ids, CSR.
- **After this:** you can model any graph in Rust. **In projects:** dependency tools.
- **Required first:** `iv-graph-ownership` (OWN 6). **Used again in:** `iv-alg-reach` (GRAPH 2).
- **Cookbook:** similar\* `csr-graph`, similar `index-graph`, related `arena-ids`. **Status:** to be written. **Runner:** gradable today.

#### GRAPH 2. Which services can this outage reach

`iv-alg-reach` · 45 min · easy · P1 · Tracks: DSA

- **New idea:** breadth and depth first visits.
- **Core:** BFS distances from a failed service.
- **Follow-up 1:** Multi-source from several failures. *Test:* per-source BFS oracle.
- **Follow-up 2:** Bounded hops and the actual path. *Test:* path validity checker.
- **Follow-up 3:** Iterative DFS with enter and exit times for subtree queries. *Test:* brute force.
- **Trade-offs:** Queue discipline. **Concepts:** BFS, DFS.
- **After this:** you can find what is affected. **In projects:** incident tools.
- **Required first:** `iv-alg-graph-repr` (GRAPH 1). **Used again in:** `iv-dataflow` (LANG 6), `iv-aho-corasick` (SEARCH 8), `iv-hnsw` (SEARCH 14), `iv-lin-graph` (LIN 7), `iv-alg-regions` (GRAPH 3), `iv-alg-order` (GRAPH 4), `iv-alg-routes` (GRAPH 5), `iv-alg-flow` (GRAPH 7).
- **Cookbook:** similar `graphs-and-union-find-questions`. **Status:** to be written. **Runner:** gradable today.

#### GRAPH 3. Failure domains on a rack grid

`iv-alg-regions` · 45 min · easy · P2 · Tracks: DSA

- **New idea:** a grid is a graph you do not store.
- **Core:** Label connected regions on a grid.
- **Follow-up 1:** Largest region after flipping one cell. *Test:* brute force.
- **Follow-up 2:** Distance to the nearest healthy cell for all cells. *Test:* per-cell BFS oracle.
- **Follow-up 3:** 0-1 costs with a deque. *Test:* Dijkstra oracle.
- **Trade-offs:** Grid as implicit graph. **Concepts:** flood fill, 0-1 BFS.
- **After this:** you can label regions. **In projects:** maps and racks.
- **Required first:** `iv-alg-reach` (GRAPH 2).
- **Cookbook:** similar `graphs-and-union-find-questions`, related `shortest-path-zoo`. **Status:** to be written. **Runner:** gradable today.

#### GRAPH 4. Plan a safe migration order

`iv-alg-order` · 60 min · medium · P1 · Tracks: DSA

- **New idea:** process a node after its prerequisites.
- **Core:** Order migrations so prerequisites come first, or report a cycle path.
- **Follow-up 1:** Smallest order among valid ones. *Test:* brute force over permutations on small inputs.
- **Follow-up 2:** Group into parallel waves. *Test:* each wave minimal, longest-path oracle.
- **Follow-up 3:** Add edges incrementally and keep a valid order. *Test:* recompute oracle.
- **Trade-offs:** Kahn vs DFS colouring. **Concepts:** topological order, cycle reporting.
- **After this:** you can order builds and migrations. **In projects:** build systems.
- **Required first:** `iv-alg-reach` (GRAPH 2). **Used again in:** `iv-lin-recompute` (LIN 8), `iv-alg-scc` (GRAPH 6).
- <details><summary>Links hidden until asked</summary>used again in: `iv-spreadsheet` (PAT 15), `iv-ssa-regalloc` (LANG 12), `iv-serializability-checker` (TXN 2), `iv-lock-manager` (TXN 4), `iv-mon-lockdep` (MON 6), `iv-alg-co-dag-dp` (COMBO 4)</details>
- **Cookbook:** similar `graphs-and-union-find-questions`, related `problem-modeling-reductions`. **Status:** to be written. **Runner:** gradable today.

#### GRAPH 5. Cheapest replication routes

`iv-alg-routes` · 60 min · medium · P2 · Tracks: DSA

- **New idea:** settle the nearest unsettled node first.
- **Core:** Shortest paths on a weighted network with path output.
- **Follow-up 1:** Per-hop limit. *Test:* Bellman-Ford oracle.
- **Follow-up 2:** Second-best route. *Test:* brute force on small graphs.
- **Follow-up 3:** A* with an admissible heuristic on a grid. *Test:* same cost, fewer nodes expanded.
- **Trade-offs:** Lazy deletion. **Concepts:** Dijkstra, heaps, heuristics.
- **After this:** you can find cheapest paths. **In projects:** routing.
- **Required first:** `iv-alg-reach` (GRAPH 2).
- <details><summary>Links hidden until asked</summary>helpful: `iv-binary-heap` (COLL 4); used again in: `iv-alg-co-graph-heap` (COMBO 5)</details>
- **Cookbook:** similar\* `dijkstra`, similar `shortest-path-zoo`. **Status:** to be written. **Runner:** gradable today.

#### GRAPH 6. Find tangled module groups

`iv-alg-scc` · 90 min · medium · P3 · Tracks: DSA

- **New idea:** low links reveal mutual reachability.
- **Core:** Strongly connected components without recursion.
- **Follow-up 1:** Condensation graph is acyclic. *Test:* checker, reachability oracle.
- **Follow-up 2:** Fewest edges to make it strongly connected. *Test:* brute force small.
- **Follow-up 3:** Bridges and articulation points in the undirected view. *Test:* removal oracle.
- **Trade-offs:** Low-link. **Concepts:** SCC, low-link, explicit stack.
- **After this:** you can find cyclic tangles. **In projects:** architecture tools.
- **Required first:** `iv-alg-order` (GRAPH 4).
- **Cookbook:** similar\* `tarjan-scc`, similar `mst-bridges`. **Status:** to be written. **Runner:** gradable today.

#### GRAPH 7. Assign tasks to workers by capacity

`iv-alg-flow` · 120 min · hard · P3 · Tracks: DSA

- **New idea:** augmenting paths push more through.
- **Core:** Maximum bipartite matching.
- **Follow-up 1:** Capacities with max flow. *Test:* brute force small.
- **Follow-up 2:** Min cut as an explanation. *Test:* cut value equals flow.
- **Follow-up 3:** Cheapest assignment. *Test:* permutation oracle.
- **Trade-offs:** Augmenting paths. **Concepts:** matching, flow.
- **After this:** you can solve assignment. **In projects:** matching systems.
- **Required first:** `iv-alg-reach` (GRAPH 2).
- **Cookbook:** similar\* `max-flow-dinic`, similar `bipartite-matching`. **Status:** to be written. **Runner:** gradable today.

### DP: Algorithms: dynamic programming

Define the state, then the transition, then reconstruct. *Algorithm series, 7 parts, about 7 h. Ends in `cap-optimizer`.* Alongside it: `iv-test-differential` (TEST 6).

**DSA overlap:** the DSA area covers 1-D and 2-D Dynamic Programming with LeetCode problems, Python templates and Learn pages (content/dsa/lessons/*.toml, `/dsa/patterns/...`). Learn the pattern there. These parts are Rust, set in engineering scenarios, with my own statements and brute-force oracles; no LeetCode statement is copied, and nothing in the Learn pages or tags changes.

#### DP 1. Cheapest retry plan

`iv-alg-dp-1d` · 45 min · easy · P1 · Tracks: DSA

- **New idea:** a state that summarises the past.
- **Core:** A one-dimensional recurrence over attempts with costs.
- **Follow-up 1:** Reconstruct the chosen plan. *Test:* cost recomputed equals optimum.
- **Follow-up 2:** A cooldown rule adds a state. *Test:* exhaustive oracle.
- **Follow-up 3:** O(1) space. *Test:* same answers.
- **Trade-offs:** Defining the state. **Concepts:** 1-D DP.
- **After this:** you can optimise sequential choices. **In projects:** retry and pricing plans.
- **Required first:** `iv-alg-regex-count` (REC 5). **Used again in:** `iv-alg-dp-grid` (DP 2), `iv-alg-dp-knapsack` (DP 3), `iv-alg-dp-lis` (DP 5), `iv-alg-dp-state-machine` (DP 6).
- <details><summary>Links hidden until asked</summary>used again in: `iv-alg-co-weighted-intervals` (COMBO 2), `iv-alg-co-dag-dp` (COMBO 4), `iv-alg-co-window-deque-dp` (COMBO 10)</details>
- **Cookbook:** similar `dp-memoization`, related `dp-zoo`. **Status:** to be written. **Runner:** gradable today.

#### DP 2. Cheapest path through a deployment grid

`iv-alg-dp-grid` · 45 min · easy · P3 · Tracks: DSA

- **New idea:** two-dimensional states.
- **Core:** Grid DP with blocked cells.
- **Follow-up 1:** k allowed overrides add a dimension. *Test:* BFS oracle.
- **Follow-up 2:** Two simultaneous walkers. *Test:* brute force small.
- **Follow-up 3:** Rolling row. *Test:* same answers.
- **Trade-offs:** State dimensions. **Concepts:** grid DP.
- **After this:** you can plan over grids. **In projects:** layout tools.
- **Required first:** `iv-alg-dp-1d` (DP 1). **Used again in:** `iv-alg-dp-strings` (DP 4), `iv-alg-dp-interval` (DP 7).
- <details><summary>Links hidden until asked</summary>used again in: `iv-alg-co-tree-dp` (COMBO 7)</details>
- **Cookbook:** similar `dp-count-paths`, related `dp-and-backtracking-questions`. **Status:** to be written. **Runner:** gradable today.

#### DP 3. Pick jobs for a maintenance window

`iv-alg-dp-knapsack` · 60 min · medium · P1 · Tracks: DSA

- **New idea:** capacity as a state dimension.
- **Core:** 0/1 knapsack over durations and values.
- **Follow-up 1:** Recover the chosen set. *Test:* subset brute force.
- **Follow-up 2:** Unbounded and bounded variants. *Test:* oracle.
- **Follow-up 3:** Meet in the middle when capacity is huge. *Test:* oracle for n = 40 against a pruned search.
- **Trade-offs:** Pseudo-polynomial. **Concepts:** knapsack.
- **After this:** you can pick under a budget. **In projects:** capacity planning.
- **Required first:** `iv-alg-dp-1d` (DP 1).
- <details><summary>Links hidden until asked</summary>used again in: `iv-alg-co-graph-heap` (COMBO 5), `iv-alg-co-bitmask-dp` (COMBO 6)</details>
- **Cookbook:** similar\* `dp-knapsack`, similar `algorithmic-optimization-toolbox`. **Status:** to be written. **Runner:** gradable today.

#### DP 4. Fuzzy-match two config files

`iv-alg-dp-strings` · 60 min · medium · P1 · Tracks: DSA

- **New idea:** align two sequences.
- **Core:** Edit distance between line lists.
- **Follow-up 1:** Produce the edit script. *Test:* applying it gives the target.
- **Follow-up 2:** Banded version for small distances. *Test:* equals full table, cells-visited bound.
- **Follow-up 3:** Linear space with reconstruction. *Test:* same cost.
- **Trade-offs:** Table orientation. **Concepts:** two-sequence DP.
- **After this:** you can compare text. **In projects:** diff tools.
- **Required first:** `iv-alg-dp-grid` (DP 2). **Used again in:** `iv-fuzzy` (SEARCH 7), `iv-diff` (LIN 1).
- **Cookbook:** similar\* `dp-edit-distance`, similar `dp-lcs-diff`. **Status:** to be written (partly practised by `y8-shortest-edit-script`). **Runner:** gradable today.

#### DP 5. Longest compatible upgrade chain

`iv-alg-dp-lis` · 60 min · medium · P2 · Tracks: DSA

- **New idea:** a smarter state with binary search.
- **Core:** Longest increasing chain in O(n log n).
- **Follow-up 1:** Return the chain. *Test:* validity and length against O(n^2).
- **Follow-up 2:** Two-key version needing a sort rule. *Test:* brute force.
- **Follow-up 3:** Minimum number of chains to cover. *Test:* oracle.
- **Trade-offs:** Patience sorting. **Concepts:** LIS, binary search.
- **After this:** you can find long chains. **In projects:** version planning.
- **Required first:** `iv-alg-dp-1d` (DP 1).
- **Cookbook:** similar `dp-zoo`. **Status:** to be written. **Runner:** gradable today.

#### DP 6. Billing with cooldowns and fees

`iv-alg-dp-state-machine` · 60 min · medium · P3 · Tracks: DSA

- **New idea:** explicit modes as states.
- **Core:** DP over explicit states for a plan with pauses.
- **Follow-up 1:** At most k switches. *Test:* exhaustive oracle.
- **Follow-up 2:** Recover the schedule. *Test:* validity checker.
- **Follow-up 3:** Enum states in Rust with exhaustive transitions. *Test:* reviewed.
- **Trade-offs:** Modelling states. **Concepts:** state machine DP.
- **After this:** you can optimise with cooldowns. **In projects:** billing.
- **Required first:** `iv-alg-dp-1d` (DP 1). **Helpful:** `iv-vending-machine` (PAT 1).
- **Cookbook:** similar `dp-and-backtracking-questions`. **Status:** to be written. **Runner:** gradable today.

#### DP 7. Merge log segments at least cost

`iv-alg-dp-interval` · 90 min · hard · P2 · Tracks: DSA

- **New idea:** solve ranges from smaller ranges.
- **Core:** Interval DP over adjacent merges.
- **Follow-up 1:** Output the merge tree. *Test:* cost recomputed.
- **Follow-up 2:** k-way merges. *Test:* brute force small.
- **Follow-up 3:** Compare with the greedy that fails. *Test:* counterexample test.
- **Trade-offs:** Split points. **Concepts:** interval DP.
- **After this:** you can plan merges. **In projects:** storage and parsing.
- **Required first:** `iv-alg-dp-grid` (DP 2).
- **Cookbook:** similar `dp-zoo` (matrix-chain). **Status:** to be written. **Runner:** gradable today.

### IVL: Algorithms: intervals

Merge, maintain, sweep and query ranges. *Algorithm series, 4 parts, about 4 h. Ends in `cap-tsdb`.* Alongside it: `iv-test-differential` (TEST 6).

**DSA overlap:** the DSA area covers Intervals with LeetCode problems, Python templates and Learn pages (content/dsa/lessons/*.toml, `/dsa/patterns/...`). Learn the pattern there. These parts are Rust, set in engineering scenarios, with my own statements and brute-force oracles; no LeetCode statement is copied, and nothing in the Learn pages or tags changes.

#### IVL 1. Merge maintenance windows

`iv-alg-iv-merge` · 45 min · easy · P1 · Tracks: DSA

- **New idea:** sort by start, then extend or emit.
- **Core:** Merge overlapping half-open windows.
- **Follow-up 1:** Touching policy and total covered time. *Test:* unit-grid oracle.
- **Follow-up 2:** Subtract blackout windows. *Test:* oracle.
- **Follow-up 3:** Streaming insert keeping merged form. *Test:* re-merge oracle.
- **Trade-offs:** Half-open vs closed. **Concepts:** sort and sweep.
- **After this:** you can merge time ranges. **In projects:** calendars.
- **Required first:** `iv-interval-booking` (COLL 10). **Used again in:** `iv-alg-iv-sweep` (IVL 2), `iv-alg-iv-set` (IVL 3), `iv-alg-gr-select` (GRD 1).
- **Cookbook:** similar `greedy-and-interval-algorithms`, similar `intervals-greedy-bit-questions`. **Status:** to be written. **Runner:** gradable today.

#### IVL 2. Peak concurrency from traces

`iv-alg-iv-sweep` · 45 min · medium · P1 · Tracks: DSA

- **New idea:** turn intervals into ordered events.
- **Core:** Sweep start and end events to find the peak and when it occurs.
- **Follow-up 1:** Time at each concurrency level. *Test:* grid oracle.
- **Follow-up 2:** Per-tenant peaks and tie order. *Test:* oracle.
- **Follow-up 3:** Minimum number of workers with an assignment. *Test:* validity checker.
- **Trade-offs:** Tie-breaking ends before starts. **Concepts:** sweep line.
- **After this:** you can compute peak load. **In projects:** capacity planning.
- **Required first:** `iv-alg-iv-merge` (IVL 1). **Used again in:** `iv-alg-iv-queries` (IVL 4).
- <details><summary>Links hidden until asked</summary>used again in: `iv-alg-co-weighted-intervals` (COMBO 2), `iv-alg-co-greedy-heap` (COMBO 3)</details>
- **Cookbook:** similar `greedy-and-interval-algorithms`. **Status:** to be written. **Runner:** gradable today.

#### IVL 3. An interval set type

`iv-alg-iv-set` · 90 min · medium · P2 · Tracks: S4

- **New idea:** maintain disjoint ranges under updates.
- **Core:** Insert and remove ranges in an ordered map keeping them disjoint.
- **Follow-up 1:** contains and next gap. *Test:* bit-vector oracle.
- **Follow-up 2:** Union, intersection and difference iterators. *Test:* oracle.
- **Follow-up 3:** O(log n + k) per op. *Test:* scale test.
- **Trade-offs:** BTreeMap neighbours. **Concepts:** ordered map ranges.
- **After this:** you can track free ranges. **In projects:** allocators and TCP.
- **Required first:** `iv-alg-iv-merge` (IVL 1). **Used again in:** `iv-tcp-reassembly` (NET 2).
- **Cookbook:** similar `std-btree-binaryheap-ordered`, related `greedy-and-interval-algorithms`. **Status:** to be written (partly practised by `s4-interval-map`). **Runner:** gradable today.

#### IVL 4. Which rules cover this address

`iv-alg-iv-queries` · 90 min · medium · P3 · Tracks: DSA

- **New idea:** sort queries along with data.
- **Core:** Offline stabbing queries by sorting both sides.
- **Follow-up 1:** Smallest covering rule per query. *Test:* scan oracle.
- **Follow-up 2:** Online version with an interval tree. *Test:* oracle.
- **Follow-up 3:** Dynamic insert and remove. *Test:* oracle.
- **Trade-offs:** Offline vs online. **Concepts:** offline sorting, interval tree.
- **After this:** you can answer many range lookups. **In projects:** firewalls.
- **Required first:** `iv-alg-iv-sweep` (IVL 2). **Helpful:** `iv-treap-order-stats` (COLL 15).
- <details><summary>Links hidden until asked</summary>used again in: `iv-alg-co-offline-dsu` (COMBO 9)</details>
- **Cookbook:** similar `treap-interval` (interval queries). **Status:** to be written. **Runner:** gradable today.

### GRD: Algorithms: greedy

Choose locally, and prove it or find the counterexample. *Algorithm series, 3 parts, about 2 h. Ends in `cap-jobsched`.* Alongside it: `iv-test-differential` (TEST 6).

**DSA overlap:** the DSA area covers Greedy with LeetCode problems, Python templates and Learn pages (content/dsa/lessons/*.toml, `/dsa/patterns/...`). Learn the pattern there. These parts are Rust, set in engineering scenarios, with my own statements and brute-force oracles; no LeetCode statement is copied, and nothing in the Learn pages or tags changes.

#### GRD 1. Book the most jobs on one runner

`iv-alg-gr-select` · 45 min · easy · P1 · Tracks: DSA

- **New idea:** an exchange argument justifies a choice.
- **Core:** Choose the maximum number of non-overlapping jobs.
- **Follow-up 1:** Show two tempting rules fail. *Test:* counterexample inputs the tests include.
- **Follow-up 2:** Minimum points that hit every window. *Test:* subset brute force.
- **Follow-up 3:** Cover a time range with the fewest shifts. *Test:* brute force.
- **Trade-offs:** Exchange argument. **Concepts:** greedy, proof by exchange.
- **After this:** you can schedule one resource. **In projects:** booking.
- **Required first:** `iv-alg-iv-merge` (IVL 1). **Used again in:** `iv-alg-gr-deadlines` (GRD 2), `iv-alg-gr-huffman` (GRD 3).
- <details><summary>Links hidden until asked</summary>used again in: `iv-alg-co-bsearch-greedy` (COMBO 1)</details>
- **Cookbook:** similar\* `greedy-and-interval-algorithms` (proof and counterexample per problem). **Status:** to be written. **Runner:** gradable today.

#### GRD 2. Schedule deploys against deadlines

`iv-alg-gr-deadlines` · 60 min · medium · P2 · Tracks: DSA

- **New idea:** keep the option to regret a choice.
- **Core:** Minimise maximum lateness.
- **Follow-up 1:** Maximise on-time count by dropping the longest. *Test:* permutation oracle.
- **Follow-up 2:** Profit with deadlines. *Test:* oracle.
- **Follow-up 3:** Explain when greedy stops working (weights). *Test:* counterexample.
- **Trade-offs:** Regret greedy. **Concepts:** greedy, heaps.
- **After this:** you can meet deadlines. **In projects:** release planning.
- **Required first:** `iv-alg-gr-select` (GRD 1).
- <details><summary>Links hidden until asked</summary>helpful: `iv-binary-heap` (COLL 4); used again in: `iv-alg-co-greedy-heap` (COMBO 3)</details>
- **Cookbook:** similar `greedy-and-interval-algorithms`, similar `heuristics-zoo`. **Status:** to be written. **Runner:** gradable today.

#### GRD 3. Order merges to minimise rewrite

`iv-alg-gr-huffman` · 45 min · easy · P3 · Tracks: DSA

- **New idea:** merge the cheapest first.
- **Core:** Always merge the two smallest runs.
- **Follow-up 1:** k-way version with padding. *Test:* brute force small.
- **Follow-up 2:** Build the code tree and decode. *Test:* round trip.
- **Follow-up 3:** Adjacent-only merges break the rule. *Test:* counterexample.
- **Trade-offs:** Why it is optimal. **Concepts:** greedy, priority queue.
- **After this:** you can plan merges. **In projects:** compaction.
- **Required first:** `iv-alg-gr-select` (GRD 1).
- <details><summary>Links hidden until asked</summary>helpful: `iv-binary-heap` (COLL 4)</details>
- **Cookbook:** similar `compression-lz77-huffman`, related `stacks-queues-heaps-questions`. **Status:** to be written. **Runner:** gradable today.

### BIT: Algorithms: bit manipulation

Words as sets, fields and tricks. *Algorithm series, 4 parts, about 4 h. Ends in `cap-fs`.* Alongside it: `iv-write-tests` (TEST 2).

**DSA overlap:** the DSA area covers Bit Manipulation with LeetCode problems, Python templates and Learn pages (content/dsa/lessons/*.toml, `/dsa/patterns/...`). Learn the pattern there. These parts are Rust, set in engineering scenarios, with my own statements and brute-force oracles; no LeetCode statement is copied, and nothing in the Learn pages or tags changes.

#### BIT 1. Permission sets in one word

`iv-alg-bit-flags` · 45 min · easy · P1 · Tracks: S8

- **New idea:** a set stored in one integer.
- **Core:** A typed flag set with set operations and iteration.
- **Follow-up 1:** Display and parse names. *Test:* round trip.
- **Follow-up 2:** Role hierarchy closure. *Test:* set oracle.
- **Follow-up 3:** const construction and operator traits. *Test:* compile-time constants.
- **Trade-offs:** Newtype over u64. **Concepts:** bit masks, operator traits.
- **After this:** you can model permissions. **In projects:** APIs.
- **Required first:** nothing. **Used again in:** `iv-bloom` (CACHE 4), `iv-alg-bit-fields` (BIT 2), `iv-alg-bit-tricks` (BIT 3), `iv-alg-bit-xor` (BIT 4).
- **Cookbook:** similar\* `bit-manipulation-real-world` (flags section). **Status:** to be written (partly practised by `f2-tcp-flags-in-a-byte`). **Runner:** gradable today.

#### BIT 2. Pack ids and headers

`iv-alg-bit-fields` · 60 min · medium · P2 · Tracks: F2

- **New idea:** several values packed with shifts and masks.
- **Core:** Pack and unpack timestamp, node and sequence into 64 bits.
- **Follow-up 1:** Order preservation. *Test:* property.
- **Follow-up 2:** CIDR containment and aggregation. *Test:* address-set oracle on small widths.
- **Follow-up 3:** Checked field overflow. *Test:* tables.
- **Trade-offs:** Shifts and masks without UB. **Concepts:** shifts, masks, CIDR.
- **After this:** you can read headers and ids. **In projects:** networking.
- **Required first:** `iv-alg-bit-flags` (BIT 1). **Used again in:** `iv-gorilla` (BIN 7), `iv-sec-egress` (SEC 7), `iv-columnar-encodings` (STORE 10), `iv-snowflake` (RESIL 9).
- **Cookbook:** similar\* `bit-manipulation-real-world`. **Status:** to be written. **Runner:** gradable today.

#### BIT 3. Word-level primitives

`iv-alg-bit-tricks` · 60 min · medium · P2 · Tracks: F5

- **New idea:** arithmetic that finds and enumerates bits.
- **Core:** Lowest set bit, next permutation of a mask, popcount, power-of-two rounding.
- **Follow-up 1:** Iterate all submasks. *Test:* enumeration oracle.
- **Follow-up 2:** Gray code and de-interleave (Morton). *Test:* round trip.
- **Follow-up 3:** select(k) in a word. *Test:* loop oracle.
- **Trade-offs:** std intrinsics vs tricks. **Concepts:** bit tricks.
- **After this:** you can iterate sets fast. **In projects:** allocators.
- **Required first:** `iv-alg-bit-flags` (BIT 1). **Used again in:** `iv-buddy` (MEM 4), `iv-bitset-roaring` (COLL 9), `iv-log-linear-histogram` (MON 9), `iv-simd-scan` (PERF 4).
- <details><summary>Links hidden until asked</summary>used again in: `iv-alg-co-bitmask-dp` (COMBO 6), `iv-alg-co-bt-bitset` (COMBO 8)</details>
- **Cookbook:** similar\* `bit-tricks`. **Status:** to be written (partly practised by `f5-bitset-iter`). **Runner:** gradable today.

#### BIT 4. Find the odd records

`iv-alg-bit-xor` · 45 min · easy · P3 · Tracks: DSA

- **New idea:** XOR cancels pairs.
- **Core:** XOR to find one or two unpaired ids in O(1) space.
- **Follow-up 1:** Every id three times but one. *Test:* bit-counting oracle.
- **Follow-up 2:** Prefix XOR range queries. *Test:* oracle.
- **Follow-up 3:** Corrupted-block recovery from parity. *Test:* round trip.
- **Trade-offs:** When XOR tricks apply. **Concepts:** XOR.
- **After this:** you can use parity. **In projects:** storage and sync.
- **Required first:** `iv-alg-bit-flags` (BIT 1). **Used again in:** `iv-disk-sched-raid` (OS 3).
- **Cookbook:** similar `intervals-greedy-bit-questions`, related `disk-scheduling-raid`. **Status:** to be written. **Runner:** gradable today.

### COMBO: Combinations: using several techniques at once

Recognise which earlier ideas a problem needs. Links are hidden until asked. *Combination series, 10 parts, about 14 h. Ends in `cap-optimizer`.* Alongside it: `iv-test-differential` (TEST 6).

**DSA overlap:** the DSA area covers the pattern pages of the parts it mixes with LeetCode problems, Python templates and Learn pages (content/dsa/lessons/*.toml, `/dsa/patterns/...`). Learn the pattern there. These parts are Rust, set in engineering scenarios, with my own statements and brute-force oracles; no LeetCode statement is copied, and nothing in the Learn pages or tags changes.

#### COMBO 1. Smallest capacity that meets the deadline

`iv-alg-co-bsearch-greedy` · 45 min · medium · P1 · Tracks: DSA

- **New idea:** search for the answer and check it greedily.
- **Core:** Find the least daily capacity that ships ordered batches in time.
- **Follow-up 1:** Prove monotonicity and add a second constraint. *Test:* linear-scan oracle.
- **Follow-up 2:** Real-valued rate to a tolerance. *Test:* oracle.
- **Follow-up 3:** Feasibility check by a heap-based greedy. *Test:* brute force.
- **Trade-offs:** Binary search on the answer. **Concepts:** binary search on the answer, greedy check.
- **After this:** you can size capacity. **In projects:** planning.
- <details><summary>Links hidden until asked</summary>required first: `iv-alg-gr-select` (GRD 1)</details>
- **Cookbook:** similar `arrays-strings-hashmap-questions` (Koko), related `sorting-searching`. **Status:** to be written. **Runner:** gradable today.

#### COMBO 2. Sell reservation slots

`iv-alg-co-weighted-intervals` · 60 min · medium · P2 · Tracks: DSA

- **New idea:** dynamic programming over sorted intervals.
- **Core:** Maximum value of non-overlapping bookings.
- **Follow-up 1:** k identical rooms. *Test:* brute force small.
- **Follow-up 2:** Reconstruct bookings. *Test:* validity.
- **Follow-up 3:** Compare with the greedy that fails. *Test:* counterexample.
- **Trade-offs:** Sort, binary search, DP. **Concepts:** DP, binary search, intervals.
- **After this:** you can sell slots optimally. **In projects:** ads and booking.
- <details><summary>Links hidden until asked</summary>required first: `iv-alg-dp-1d` (DP 1), `iv-alg-iv-sweep` (IVL 2)</details>
- **Cookbook:** similar `dp-zoo` (weighted interval scheduling). **Status:** to be written. **Runner:** gradable today.

#### COMBO 3. Assign jobs to a changing pool

`iv-alg-co-greedy-heap` · 60 min · medium · P1 · Tracks: DSA

- **New idea:** a sweep with queues of free and busy.
- **Core:** Process interval jobs in time order and keep free workers and busy workers ready.
- **Follow-up 1:** Priorities and preemption. *Test:* brute-force simulator.
- **Follow-up 2:** Maximise profit by replacing the weakest accepted job. *Test:* subset oracle.
- **Follow-up 3:** Cooldown per worker. *Test:* oracle.
- **Trade-offs:** Two heaps. **Concepts:** greedy, two priority queues, sweep.
- **After this:** you can assign work online. **In projects:** dispatch.
- <details><summary>Links hidden until asked</summary>required first: `iv-alg-iv-sweep` (IVL 2), `iv-binary-heap` (COLL 4); helpful: `iv-alg-gr-deadlines` (GRD 2)</details>
- **Cookbook:** similar `greedy-and-interval-algorithms`, related `heap-patterns`. **Status:** to be written. **Runner:** gradable today.

#### COMBO 4. Critical path of a build graph

`iv-alg-co-dag-dp` · 90 min · medium · P1 · Tracks: DSA

- **New idea:** dynamic programming in dependency order.
- **Core:** Longest path and slack over an acyclic build graph.
- **Follow-up 1:** Count the number of critical paths without overflow surprises. *Test:* enumeration oracle.
- **Follow-up 2:** Recompute only what changes after one duration edit. *Test:* recompute-all oracle, work counter.
- **Follow-up 3:** Best set of k tasks to speed up on small graphs. *Test:* brute force.
- **Trade-offs:** DP order is the topological order. **Concepts:** DAG DP, topological order, incremental recomputation.
- **After this:** you can find the critical path of a build. **In projects:** CI.
- <details><summary>Links hidden until asked</summary>required first: `iv-alg-order` (GRAPH 4), `iv-alg-dp-1d` (DP 1); helpful: `iv-lin-recompute` (LIN 8)</details>
- **Cookbook:** similar `dp-count-paths`. **Status:** to be written. **Runner:** gradable today.

#### COMBO 5. Route with budgets and fuel

`iv-alg-co-graph-heap` · 90 min · hard · P2 · Tracks: DSA

- **New idea:** the state includes what you have spent.
- **Core:** Shortest path over states (node, remaining budget).
- **Follow-up 1:** Dominance pruning. *Test:* states-expanded bound.
- **Follow-up 2:** Refuel stops. *Test:* exhaustive oracle.
- **Follow-up 3:** k shortest distinct routes. *Test:* brute force.
- **Trade-offs:** State-space graphs. **Concepts:** Dijkstra on states, heaps.
- **After this:** you can route with budgets. **In projects:** logistics.
- <details><summary>Links hidden until asked</summary>required first: `iv-alg-routes` (GRAPH 5), `iv-alg-dp-knapsack` (DP 3)</details>
- **Cookbook:** similar `shortest-path-zoo`, related `problem-modeling-reductions`. **Status:** to be written. **Runner:** gradable today.

#### COMBO 6. Assign test shards to machines

`iv-alg-co-bitmask-dp` · 90 min · hard · P1 · Tracks: DSA

- **New idea:** a subset as a DP state.
- **Core:** Minimise the longest machine time for up to 16 shards.
- **Follow-up 1:** Submask enumeration over machines. *Test:* permutation oracle.
- **Follow-up 2:** Required-together and forbidden-together pairs. *Test:* oracle.
- **Follow-up 3:** Reconstruct the assignment. *Test:* validity.
- **Trade-offs:** 3^n vs n*2^n. **Concepts:** bitmask DP, submasks.
- **After this:** you can optimise small assignments. **In projects:** test sharding.
- **Used again in:** `iv-join-order` (QUERY 13).
- <details><summary>Links hidden until asked</summary>required first: `iv-alg-dp-knapsack` (DP 3), `iv-alg-bit-tricks` (BIT 3)</details>
- **Cookbook:** similar `dp-zoo` (Held-Karp), related `algorithmic-optimization-toolbox`. **Status:** to be written. **Runner:** gradable today.

#### COMBO 7. Place caches in a distribution tree

`iv-alg-co-tree-dp` · 90 min · hard · P2 · Tracks: DSA

- **New idea:** combine child answers, then reroot.
- **Core:** Tree DP choosing nodes to cover requests at least cost.
- **Follow-up 1:** Reroot to get the answer for every root. *Test:* per-root oracle.
- **Follow-up 2:** At most k caches. *Test:* subset brute force.
- **Follow-up 3:** Iterative post-order for deep trees. *Test:* scale test.
- **Trade-offs:** State per child merge. **Concepts:** tree DP, rerooting.
- **After this:** you can place resources in hierarchies. **In projects:** CDNs.
- <details><summary>Links hidden until asked</summary>required first: `iv-alg-recursion-fs` (REC 1), `iv-alg-dp-grid` (DP 2)</details>
- **Cookbook:** similar `dp-zoo` (tree independent set). **Status:** to be written. **Runner:** gradable today.

#### COMBO 8. Solve a placement puzzle fast

`iv-alg-co-bt-bitset` · 90 min · hard · P2 · Tracks: DSA

- **New idea:** backtracking with candidate sets as bits.
- **Core:** Place workloads on hosts with anti-affinity using bit sets for candidates.
- **Follow-up 1:** Choose the most constrained next. *Test:* node-count bound.
- **Follow-up 2:** Count solutions with symmetry breaking. *Test:* brute force small.
- **Follow-up 3:** Undo with a trail instead of cloning. *Test:* allocation counting.
- **Trade-offs:** Bitsets vs HashSet. **Concepts:** backtracking, bitsets, propagation.
- **After this:** you can solve placement fast. **In projects:** cluster scheduling.
- <details><summary>Links hidden until asked</summary>required first: `iv-alg-rota` (REC 4), `iv-alg-bit-tricks` (BIT 3)</details>
- **Cookbook:** similar `backtracking-branch-bound`, similar `sat-solver-cdcl`. **Status:** to be written. **Runner:** gradable today.

#### COMBO 9. Connectivity over a history of link changes

`iv-alg-co-offline-dsu` · 120 min · hard · P2 · Tracks: DSA

- **New idea:** reorder queries so merging suffices.
- **Core:** Answer connectivity queries when edges are only added, by sorting queries by threshold.
- **Follow-up 1:** Weighted threshold queries. *Test:* BFS oracle.
- **Follow-up 2:** Edge removals handled offline over time segments. *Test:* BFS oracle.
- **Follow-up 3:** Minimum spanning forest by sorting. *Test:* Prim oracle.
- **Trade-offs:** Offline processing. **Concepts:** offline sorting, disjoint sets, rollback.
- **After this:** you can answer historic connectivity. **In projects:** network analysis.
- <details><summary>Links hidden until asked</summary>required first: `iv-union-find` (COLL 7); helpful: `iv-alg-iv-queries` (IVL 4)</details>
- **Cookbook:** similar `mst-bridges`, related `union-find`. **Status:** to be written. **Runner:** gradable today.

#### COMBO 10. Throughput-limited batching

`iv-alg-co-window-deque-dp` · 90 min · hard · P3 · Tracks: DSA

- **New idea:** speed a transition with a window structure.
- **Core:** DP whose transition looks at a sliding window of earlier states.
- **Follow-up 1:** Monotonic deque to reach O(n). *Test:* O(nk) oracle and scale test.
- **Follow-up 2:** Range-minimum structure alternative. *Test:* same answers.
- **Follow-up 3:** Reconstruct batches. *Test:* validity.
- **Trade-offs:** Optimising transitions. **Concepts:** DP, monotonic deque, range queries.
- **After this:** you can optimise throughput DPs. **In projects:** batching.
- <details><summary>Links hidden until asked</summary>required first: `iv-alg-dp-1d` (DP 1), `iv-minstack-queue` (COLL 1)</details>
- **Cookbook:** similar `algorithmic-optimization-toolbox`. **Status:** to be written. **Runner:** gradable today.

## 6. Views by time box

The same parts, by estimate. P1 first within each series line.

### 45 minutes (74 parts)

- **OWN:** split-borrow-bank (#1, P2)
- **TRAIT:** heterogeneous-store (#4, P3)
- **ERR:** diagnostics-renderer (#1, P2)
- **TEXT:** base64 (#3, P2); unicode-search-keys (#6, P2); text-justify (#4, P3)
- **SMART:** ambient-context (#4, P2)
- **PAT:** vending-machine (#1, P1); url-shortener (#2, P1); snake (#3, P3)
- **UNSAFE:** layout-quiz-codec (#1, P3)
- **MEM:** slab (#1, P1)
- **COLL:** minstack-queue (#1, P1); interval-booking (#10, P1)
- **CACHE:** hit-counter (#3, P1); bloom (#4, P2); reservoir (#7, P3)
- **CONC:** print-in-order (#1, P2); chunk-queue (#20, P2); dining (#4, P3); oneshot-thread (#8, P3)
- **ATOM:** spinlock (#1, P1); rcu-snapshot (#3, P2)
- **ASYNC:** send-across-await (#4, P2); async-sync-bridge (#14, P2); async-trait-object (#11, P3)
- **PARSE:** rpn-calc (#1, P1); tokenizer-borrowed (#2, P1); regex-syntax (#20, P2); semver (#5, P3); datetime-parse (#9, P3)
- **LANG:** constant-fold (#2, P2)
- **BIN:** varint-codec (#1, P1); length-codec (#2, P1); checksums (#12, P2)
- **IO:** log-analyzer (#1, P2); io-adapters (#4, P2); positional-block-io (#8, P2)
- **OS:** page-replacement (#2, P2); disk-sched-raid (#3, P3)
- **WEB:** cursor-pagination (#2, P2); http-range (#9, P2)
- **SEC:** sec-safe-path (#1, P1); sec-ct-compare (#3, P2); authz (#6, P3)
- **SBX:** sbx-command-policy (#2, P3)
- **SEARCH:** knn (#9, P2)
- **RESIL:** retry-backoff (#1, P1); snowflake (#9, P2); gcra-equivalence (#11, P2)
- **DIST:** cdc (#14, P3)
- **CRDT:** crdt-counters (#1, P1)
- **LIN:** lin-upcasting (#6, P3)
- **MON:** atomic-counters (#1, P2); mon-flight-recorder (#3, P2)
- **TEST:** fake-clock (#1, P1); seeded-rng (#8, P2)
- **NUM:** float-robust (#4, P2); camera-rays (#5, P2); mesh-subdivision (#9, P2)
- **REC:** alg-recursion-fs (#1, P2); alg-ast-walk (#2, P2)
- **GRAPH:** alg-graph-repr (#1, P1); alg-reach (#2, P1); alg-regions (#3, P2)
- **DP:** alg-dp-1d (#1, P1); alg-dp-grid (#2, P3)
- **IVL:** alg-iv-merge (#1, P1); alg-iv-sweep (#2, P1)
- **GRD:** alg-gr-select (#1, P1); alg-gr-huffman (#3, P3)
- **BIT:** alg-bit-flags (#1, P1); alg-bit-xor (#4, P3)
- **COMBO:** alg-co-bsearch-greedy (#1, P1)

### 60 minutes (190 parts)

- **OWN:** undo-stack (#4, P1); graph-ownership (#6, P1); linked-list-safe (#2, P2); view-splitter (#3, P2); text-gap-buffer (#5, P3)
- **ITER:** iter-adapters (#1, P1); fn-pipeline (#2, P2); memoize (#3, P2); peekmore-chunks (#4, P3)
- **TRAIT:** event-bus-any (#5, P1); macro-hashmap (#6, P1); shape-registry (#1, P2); units-newtypes (#2, P2); typestate-conn (#7, P2); generic-matrix (#3, P3); library-api (#8, P3)
- **ERR:** error-layers (#2, P1); panic-boundary (#3, P2)
- **TEXT:** utf8-codec (#1, P2); string-builder (#2, P2)
- **SMART:** raii-pool (#1, P1); my-refcell (#2, P1)
- **PAT:** parking-lot (#5, P1); bank-ledger (#6, P1); file-system-tree (#11, P1); splitwise (#7, P2); feature-flags (#8, P2); logger (#9, P2); notification (#10, P2); board-game (#4, P3)
- **UNSAFE:** ffi-wrapper (#3, P2); pin-mapfuture (#7, P2); nostd-ring (#2, P3)
- **COLL:** binary-heap (#4, P1); trie (#6, P1); union-find (#7, P1); leaderboard (#11, P1); ring-deque (#2, P2); median-stream (#5, P2)
- **CACHE:** lru (#1, P1); ttl-kv (#2, P1); count-min (#5, P2); sampled-expiry (#10, P2); hll (#6, P3)
- **CONC:** blocking-queue (#2, P1); bank-transfer-concurrent (#3, P1); semaphore (#5, P1); channel-mpsc (#7, P1); thread-pool (#9, P1); sharded-map (#10, P1); singleflight (#11, P1); pubsub (#12, P1); job-scheduler (#15, P1); barrier-latch (#6, P2); parallel-mapreduce (#13, P2); pipeline-stages (#14, P2)
- **ATOM:** once (#2, P2); seqlock (#6, P3)
- **ASYNC:** block-on (#1, P1); fanout-limit (#5, P1); cancel-safety (#6, P1); graceful-shutdown (#7, P1); stream-adapters (#8, P2)
- **PARSE:** csv (#3, P1); glob (#10, P1); recursive-descent (#11, P1); pratt (#12, P1); ini-toml (#4, P2); url-parser (#6, P2); cli-args (#7, P2); shell-line (#8, P2); stmt-parser-desugar (#19, P2)
- **LANG:** typecheck-simple (#3, P2); vm-local-slots (#15, P2)
- **BIN:** serde-wire (#3, P1); resp (#4, P2); png-container (#13, P2); protobuf-wire (#5, P3); packet-parse (#6, P3); gorilla (#7, P3)
- **IO:** bounded-line-reader (#2, P1); bufreader (#3, P1); atomic-file (#5, P2); process-pipeline (#7, P2); dir-walker (#6, P3)
- **OS:** cpu-scheduler (#1, P1); page-table (#4, P3)
- **NET:** tcp-reassembly (#2, P2); tcp-lingering-close (#12, P2); tokio-tcp-server (#13, P2); websocket-frames (#4, P3)
- **WEB:** router (#1, P1); sse-replay (#7, P2); axum-request-tracing (#8, P2); http-cache (#5, P3); webhook-sender (#6, P3)
- **SEC:** sec-injection (#2, P2); jwt-hmac (#4, P2); sessions (#5, P2); sec-egress (#7, P3)
- **SBX:** sbx-capabilities (#1, P2); sbx-quota (#3, P2)
- **STORE:** wal (#2, P1); slotted-page (#3, P1); memtable-flush (#5, P1); compaction (#6, P1); row-serialization (#1, P2); lru-k (#8, P2); effect-log (#17, P2); group-commit (#18, P2); bitmap-index (#11, P3)
- **QUERY:** volcano (#2, P1); external-sort (#3, P1); expr-eval-3vl (#1, P2); merge-join (#6, P2); predicate-selectivity (#14, P2); window-functions (#10, P3); asof-join (#11, P3); cardinality (#12, P3)
- **TXN:** versioned-map (#1, P1); serializability-checker (#2, P2); occ (#5, P2); timestamp-ordering (#6, P3)
- **SEARCH:** inverted-index (#1, P1); autocomplete (#6, P1); posting-compression (#2, P2); boolean-query (#3, P2); trigram (#4, P2); bm25 (#5, P2); fuzzy (#7, P2); qgram-typo-filter (#15, P2); positional-phrase (#16, P2); lsh-dedupe (#10, P3); spatial-grid (#11, P3)
- **RESIL:** circuit-breaker (#3, P1); idempotency-store (#4, P1); load-balancer (#5, P1); service-registry (#6, P2); outbox (#7, P2); saga (#8, P2); window-limiters (#10, P2)
- **DIST:** dist-delivery (#2, P1); visibility-queue (#3, P1); dist-heartbeat (#4, P1); lease-fencing (#7, P1); consistent-hash (#8, P1); lamport-vclock (#6, P2); raft-apply (#20, P2)
- **CRDT:** crdt-registers (#2, P1); crdt-set (#3, P1)
- **LIN:** lin-event-store (#4, P1); lin-graph (#7, P1); diff (#1, P2); merkle (#2, P2); lin-projections (#5, P2)
- **MON:** mon-metrics (#2, P1); mon-spans (#4, P2); mon-lockdep (#6, P2); log-linear-histogram (#9, P2); mon-slo (#5, P3); mon-flame (#7, P3); mon-anomaly (#8, P3)
- **TEST:** write-tests (#2, P1); shrinker (#3, P1); test-differential (#6, P1); proptest-use (#4, P2); fuzz-harness (#5, P2); test-failpoints (#7, P2); disk-fault-model (#10, P2); seeded-data-gen (#11, P2); sim-invariant-search (#12, P2)
- **PERF:** alloc-budget (#2, P1); cache-friendly (#3, P2); bench-harness (#1, P3)
- **NUM:** direct-shading (#6, P2); surface-optics (#7, P2); raster (#2, P3)
- **REC:** alg-config-matrix (#3, P2); alg-regex-count (#5, P3)
- **GRAPH:** alg-order (#4, P1); alg-routes (#5, P2)
- **DP:** alg-dp-knapsack (#3, P1); alg-dp-strings (#4, P1); alg-dp-lis (#5, P2); alg-dp-state-machine (#6, P3)
- **GRD:** alg-gr-deadlines (#2, P2)
- **BIT:** alg-bit-fields (#2, P2); alg-bit-tricks (#3, P2)
- **COMBO:** alg-co-greedy-heap (#3, P1); alg-co-weighted-intervals (#2, P2)

### 90 minutes (108 parts)

- **OWN:** scoped-callbacks (#7, P2); lending-windows (#8, P2)
- **ITER:** flatten-tree-iter (#5, P2)
- **TRAIT:** manual-vtable (#9, P3)
- **TEXT:** template-engine (#5, P2)
- **PAT:** ticket-booking (#13, P1); elevator (#12, P2)
- **UNSAFE:** fixed-vec (#4, P2)
- **MEM:** bump-arena (#2, P1); buddy (#4, P2)
- **COLL:** hashmap-chaining (#3, P1); indexed-pq (#8, P1); bitset-roaring (#9, P2); skiplist-spans (#18, P2)
- **CACHE:** lfu (#8, P1); generation-cache (#9, P2)
- **CONC:** actor (#16, P2); rwlock (#17, P2); timing-wheel (#18, P2); parked-waiters (#21, P2); park-wake-handshake (#22, P2)
- **ATOM:** arc (#4, P1); spsc (#5, P1); treiber (#8, P2)
- **ASYNC:** executor (#2, P1); async-channel (#3, P1); async-batcher (#9, P2); async-retry-hedge (#12, P2); tokio-job-outcome (#13, P2); task-wake-state (#15, P2); future-combinators (#16, P2); register-recheck (#17, P2)
- **PARSE:** parser-combinators (#16, P2); json-streaming (#17, P2); error-recovery (#18, P2); nfa-anchors-search (#21, P2); cps-backtracker (#22, P2)
- **LANG:** sexpr-lisp (#1, P2); interp-call-frames (#13, P2); interp-object-model (#14, P2); dataflow (#6, P3); peg-packrat (#7, P3); formatter (#8, P3)
- **BIN:** lz77 (#8, P3); huffman (#9, P3)
- **OS:** inode-block-map (#7, P2); unix-namespace (#8, P2)
- **NET:** tcp-echo-framed (#5, P1); sans-io (#1, P2); mio-reactor-api (#10, P2); http-response-framing (#11, P2); reliable-udp (#3, P3)
- **WEB:** middleware-tower (#4, P2)
- **SEC:** sec-passwords (#8, P3)
- **SBX:** sbx-fuel (#5, P1); sbx-vfs-jail (#4, P2); sbx-supervisor (#6, P2)
- **STORE:** heap-file (#4, P2); columnar-encodings (#10, P2); extendible-hash (#12, P2); online-log-rewrite (#19, P2); crash-ordered-files (#20, P2)
- **QUERY:** hash-join (#5, P1); hash-agg (#4, P2); planner-rules (#9, P2); interesting-orders (#15, P2)
- **SEARCH:** aho-corasick (#8, P2); immutable-segments (#17, P2); suffix-array (#12, P3); ivf (#13, P3)
- **RESIL:** aimd-concurrency (#12, P2)
- **DIST:** raft-election (#18, P1); sim-network (#1, P2); stream-windows (#13, P2); raft-core (#17, P2); raft-persist (#21, P2); dist-sharding (#9, P3); dist-anti-entropy (#11, P3); raft-snapshot (#22, P3); raft-reads (#23, P3)
- **CRDT:** crdt-convergence (#4, P1); crdt-causal-context (#5, P2); crdt-orswot (#6, P2); crdt-map (#7, P2); crdt-delta (#8, P2); crdt-opbased (#9, P2); crdt-gc (#10, P3); crdt-undo (#13, P3)
- **LIN:** lin-recompute (#8, P2); lin-column (#9, P2); decide-evolve (#10, P2)
- **TEST:** grammar-program-gen (#9, P2)
- **PERF:** simd-scan (#4, P3)
- **NUM:** ray-intersection (#3, P2); monte-carlo (#8, P2); bigint (#1, P3)
- **REC:** alg-rota (#4, P2); alg-memo-resolver (#6, P2)
- **GRAPH:** alg-scc (#6, P3)
- **DP:** alg-dp-interval (#7, P2)
- **IVL:** alg-iv-set (#3, P2); alg-iv-queries (#4, P3)
- **COMBO:** alg-co-dag-dp (#4, P1); alg-co-bitmask-dp (#6, P1); alg-co-graph-heap (#5, P2); alg-co-tree-dp (#7, P2); alg-co-bt-bitset (#8, P2); alg-co-window-deque-dp (#10, P3)

### 120 minutes (63 parts)

- **TRAIT:** derive-builder (#10, P3)
- **SMART:** my-rc (#3, P1)
- **PAT:** order-book (#14, P1); spreadsheet (#15, P1)
- **UNSAFE:** myvec (#5, P1); intrusive-list (#6, P2)
- **MEM:** free-list-alloc (#3, P2); mark-sweep (#5, P2)
- **COLL:** range-queries (#12, P1); bptree-memory (#16, P1); ordered-map-skiplist (#14, P2); bvh-flat (#17, P2); cuckoo (#13, P3); treap-order-stats (#15, P3)
- **CONC:** work-stealing (#19, P2)
- **ATOM:** mpmc (#7, P2); interleaving-checker (#9, P3)
- **ASYNC:** async-primitives (#10, P2)
- **PARSE:** json (#13, P1); regex-nfa (#15, P1); markdown (#14, P2)
- **LANG:** tree-interpreter (#4, P1); bytecode-vm (#5, P1); hindley-milner (#9, P2); vm-upvalues (#16, P2); unification (#10, P3); wasm-interp (#11, P3); ssa-regalloc (#12, P3)
- **BIN:** dns-codec (#10, P3); serde-format (#11, P3)
- **OS:** fs-blocks (#5, P3); io-uring-model (#6, P3)
- **NET:** http-request (#6, P1); http-server (#7, P2); tokio-tcp-conn (#9, P2); event-loop (#8, P3)
- **WEB:** axum-api (#3, P1)
- **STORE:** bloom-sstable (#7, P1); buffer-pool (#9, P1); disk-bptree (#13, P1); checkpoint-recovery (#14, P2); redo-journal (#16, P2); latch-crabbing (#15, P3)
- **QUERY:** sql-parser (#7, P2); mini-sql-exec (#8, P2); join-order (#13, P2)
- **TXN:** mvcc-si (#3, P1); lock-manager (#4, P1)
- **SEARCH:** hnsw (#14, P3)
- **RESIL:** rate-limiter (#2, P1)
- **DIST:** raft-replication (#19, P1); quorum-kv (#10, P2); partitioned-log (#12, P2); two-phase-commit (#15, P2); gossip-failure (#5, P3); linearizability (#16, P3)
- **CRDT:** text-crdt (#11, P3); crdt-text-fugue (#12, P3); crdt-movable-tree (#14, P3); ot (#15, P3)
- **LIN:** git-objects (#3, P2)
- **GRAPH:** alg-flow (#7, P3)
- **COMBO:** alg-co-offline-dsu (#9, P2)

### Suggested order for interview preparation

If the goal in the next weeks is interviews, and not the whole path, take these in order. This is my judgement. Do each part's required parts first.

1. **Collections and caches:** binary-heap, indexed-pq, trie, union-find, hashmap-chaining, lru, ttl-kv, lfu, interval-booking, range-queries.
2. **Concurrency:** blocking-queue, bank-transfer-concurrent, semaphore, thread-pool, singleflight, sharded-map, job-scheduler, spinlock, arc, spsc.
3. **Design in code:** rate-limiter, circuit-breaker, pubsub, visibility-queue, idempotency-store, order-book, bank-ledger, parking-lot, logger, spreadsheet.
4. **Language and library depth:** iter-adapters, my-refcell, my-rc, shape-registry, typestate-conn, error-layers, macro-hashmap, myvec.
5. **Async:** block-on, executor, fanout-limit, cancel-safety, graceful-shutdown.
6. **By role, parsers:** tokenizer-borrowed, recursive-descent, pratt, json, regex-nfa. **Storage:** wal, slotted-page, buffer-pool, compaction, volcano, mvcc-si, lock-manager. **Distributed:** sim-network, lease-fencing, quorum-kv, raft-election.

## 7. Capstones and readiness

**Can it start as independent stages today?** The app cannot carry your code from one stage to the next (RUST_PRACTICE.md section 7.3). An independent stage means: its own problem, whose starter holds a working version of the earlier milestones, in one `lib.rs`. That works for most in-memory milestones but the starters grow with the project, and for the 2,000 to 5,000 line files they get unwieldy: build those in your own repository or the Courses engine. Each entry says what is blocked.

**Cookbook code may be adapted or copied** into reference solutions, tests and oracles (the owner's decision). Passing `cookbook/check.sh` does not prove an adapted exercise's contract, so each one still needs its own verification, and a starter must not carry the copied solution. The bug notes below matter for that: none was confirmed by running code.

**Readiness check.** For each capstone, every readiness row names a catalogue part. A script checked that each row resolves to a part on the roadmap, that each row's milestone exists, and that the capstone's place comes after all of them and after every part of the series that end in it. The place is computed from the rows, so that last condition holds by construction; what the check really proves is that no row is left without a part. Whether a part truly teaches the skill is the reader's judgement.

| Capstone | Source | Milestones (core / optional) | Core hours (est.) | Readiness rows: exists / staged / to write | Independent stages today |
|---|---|---|---|---|---|
| `cap-ledger` Event-sourced bank ledger | cookbook, read in full | 13 / 1 | 13 | 2 / 3 / 16 | yes: the clock, scheduler and "file" can be byte buffers |
| `cap-jobsvc` Job service over axum | cookbook, read in full | 10 / 1 | 10 | 1 / 6 / 10 | partly: axum and tower oneshot are vendored; needs several modules in one lib.rs; no sockets needed |
| `cap-lease` Lease and fencing simulator | cookbook, read in full | 5 / 1 | 5 | 0 / 0 / 6 | yes |
| `cap-runtime` Mini async runtime | cookbook, read in full | 17 / 3 | 21 | 10 / 1 / 20 | stages 1 to 5 yes; reactor stage needs mio (not vendored); work stealing can use crossbeam |
| `cap-httpserver` HTTP server on a thread pool | cookbook, read in full | 15 / 5 | 15 | 7 / 0 / 20 | parser, router, files-in-memory stages yes; server stages need loopback in the sandbox (not verified); tokio comparison optional |
| `cap-kvstore` LSM key-value store | cookbook, read in full | 8 / 2 | 10 | 3 / 3 / 8 | yes with an in-memory file trait; real files need a writable tmp (not verified) |
| `cap-redis` Mini Redis | cookbook, read in full | 29 / 3 | 34 | 5 / 4 / 36 | engine stages yes; TCP, AOF files and threads over sockets need loopback and a writable directory (not verified) |
| `cap-optimizer` Query optimizer | cookbook, read in full | 14 / 4 | 17 | 0 / 0 / 23 | yes |
| `cap-ratelimitlab` Rate limiter lab | cookbook, read in full | 7 / 1 | 6 | 2 / 2 / 7 | yes (time is an argument) |
| `cap-raytracer` Ray tracer | cookbook, read in full | 16 / 2 | 18 | 0 / 0 / 25 | math and image stages yes; rendering is slow in a debug test run (15 s cap), so tests must use tiny images |
| `cap-regex` Regex engine | cookbook, read in full | 5 / 3 | 6 | 1 / 1 / 7 | yes |
| `cap-search` Search engine | cookbook, read in full | 8 / 1 | 10 | 5 / 2 / 9 | yes |
| `cap-sql` Tiny SQL engine | cookbook, read in full | 9 / 2 | 11 | 1 / 2 / 10 | yes |
| `cap-fs` Toy filesystem | cookbook, read in full | 18 / 2 | 23 | 1 / 2 / 15 | yes with a simulated block device; real-file stage needs a writable tmp |
| `cap-lox` Interpreter with a bytecode VM | cookbook, read in full | 17 / 2 | 24 | 2 / 1 / 21 | yes, but large for one lib.rs |
| `cap-typeahead` Typeahead service | cookbook, read in full | 12 / 3 | 13 | 2 / 5 / 11 | engine stages yes; HTTP stage needs loopback or oneshot |
| `cap-pebble` PEBBLE compiler front to back | multi-file, README only | 6 outline stages | not estimated | 1 / 0 / 3 (provisional, per part) | no: 57 files and about 21,500 lines; needs multi-file projects (Courses engine) |
| `cap-pebbletools` Formatter, highlighter and language server | multi-file, README only | 5 outline stages | not estimated | 1 / 0 / 2 (provisional, per part) | no: multi-file, depends on the compiler crate, speaks over stdio |
| `cap-backend` Multi-tenant SaaS backend | multi-file, README only | 6 outline stages | not estimated | 0 / 2 / 4 (provisional, per part) | no: 49 files, sockets and a build script; Courses engine |
| `cap-vectordb` Mini vector database | multi-file, README only | 8 outline stages | not estimated | 1 / 1 / 3 (provisional, per part) | core stages yes with in-memory files; size (3,756 lines) argues for a multi-file project |
| `cap-database` Build your own database | composed, outline | 7 outline stages | not estimated | 1 / 1 / 6 (provisional, per part) | early stages yes; the whole thing is multi-file (Courses; the BusTub course already covers much of it) |
| `cap-gitstore` Git-like object store | composed, outline | 7 outline stages | not estimated | 1 / 0 / 4 (provisional, per part) | yes in memory (sha2 vendored; Git's own SHA-1 format and zlib are not covered) |
| `cap-shell` Unix shell | composed, outline | 6 outline stages | not estimated | 1 / 0 / 2 (provisional, per part) | only parsing and expansion today; execution needs processes and binaries in the sandbox |
| `cap-loadbalancer` Load balancer | composed, outline | 6 outline stages | not estimated | 1 / 0 / 3 (provisional, per part) | policy stages yes; proxy stages need loopback |
| `cap-queue` Message queue broker | composed, outline | 8 outline stages | not estimated | 2 / 0 / 4 (provisional, per part) | stages 1 to 6 yes in memory; network stage needs loopback |
| `cap-raftkv` Raft key-value store | composed, outline | 8 outline stages | not estimated | 0 / 0 / 8 (provisional, per part) | yes on a simulator; large for one lib.rs |
| `cap-jobsched` Job scheduler without containers | composed, outline | 7 outline stages | not estimated | 1 / 0 / 6 (provisional, per part) | yes with a fake clock and fake workers; real isolation cannot be graded |
| `cap-interp` Compiler to bytecode for your own language | composed, outline | 6 outline stages | not estimated | 1 / 1 / 4 (provisional, per part) | yes up to a size limit |
| `cap-collab` Collaborative block editor core | composed, outline | 6 outline stages | not estimated | 0 / 0 / 6 (provisional, per part) | yes (replicas in memory) |
| `cap-lineage` Workflow lineage and recompute engine | composed, outline | 6 outline stages | not estimated | 0 / 0 / 5 (provisional, per part) | yes; the Cookbook has no page about lineage itself |
| `cap-tsdb` Time-series database | composed, outline | 5 outline stages | not estimated | 1 / 1 / 3 (provisional, per part) | core yes in memory |
| `cap-unsafekit` Your own unsafe containers crate | composed, outline | 6 outline stages | not estimated | 4 / 0 / 2 (provisional, per part) | yes for behaviour; soundness needs Miri and ordering needs loom, neither available |
| **FlowForge** (its own path) | the owner's separate flowforge repository; plan in its `SERIES.md` | not mapped here | not estimated | not mapped | not applicable: built in its own repository |

### The 16 Cookbook capstones

#### `cap-ledger`: Event-sourced bank ledger

Data and history · hard · P2 · Cookbook: exact `event-sourced-ledger`. Existing practice: none. Today: yes: the clock, scheduler and "file" can be byte buffers.
Place: after roadmap position 383 of 435, `iv-seeded-rng` (TEST 8), in the branch order. Series ending here: PAT.

**Whole file read: yes.**

Lines read: `event_sourced_ledger.rs` 1454 of 1454.

**Milestones**

| # | Milestone | Min | Core? | Adds | Tested by | Runner lacks | Cookbook lines |
|---|---|---|---|---|---|---|---|
| m1 | Account aggregate: decide and evolve | 60 | core | Event, Command, DomainError with thiserror; pure decide and total evolve for open, deposit, withdraw | given-events, when-command, then-events tables; each domain error; evolve folds to the expected balance | needs: none | 81-207, 244-269 |
| m2 | Transfers and compensating reversal | 60 | core | TransferredOut and TransferredIn, receive never refuses, Reverse validated from remembered deltas, no overdraft on undo | reverse twice fails, reverse of a non-reversible version fails, undoing a spent deposit is refused; seeded commands never make a negative balance | needs: none | 160-163, 208-243, 261-267 |
| m3 | Append-only event store in memory | 60 | core | stored envelope, serde tagged JSON, EventStore trait, global log plus per-stream index, expected-version conflict, prepare then commit | stale expected version conflicts and changes nothing; positions and versions are gapless; a batch is all or nothing | needs: serde, serde_json, thiserror | 273-302, 348-456 |
| m4 | Command handling, idempotency and time travel | 60 | core | Ledger load by fold, try_commit separated from load, retry on conflict, command ids kept in state, state_at | same id twice gives Duplicate with no new events; two stale loads: second gets conflict then succeeds after reload; state_at per version gives known balances | needs: none | 340-346, 567-600, 615-665 |
| m5 | Snapshots | 45 | core | snapshot when the version crosses a multiple of N; load from snapshot plus tail | snapshot plus tail equals full replay on seeded histories; snapshot version is as expected and is used | needs: none | 585-613, 646-649 |
| m6 | Event schema upcasting | 45 | core | schema number per stored event, one step per version on JSON Value, decode on read, stored payload untouched | hand-written v1 payloads decode to current events; stored JSON and schema are unchanged; mixed-version stream folds correctly | needs: none | 304-339 |
| m7 | CRC frames and tail scanning | 60 | core | CRC-32, length plus checksum frames, scan returning records, valid length and tail kind | CRC check value; cut a frame sequence at every byte offset and flip bytes: only whole good frames are returned | needs: none | 63-72, 458-486 |
| m8 | Durable file store and recovery | 90 | core | write-ahead append of one frame per batch, optional sync_data, open that replays and truncates a bad tail | reopen returns the same log; truncated file keeps an exact prefix and accepts new appends that survive another reopen; flipped byte is reported | needs: none (files in /tmp; durability itself cannot be tested on tmpfs) | 488-563 |
| m9 | Read models with checkpoints | 60 | core | balances, daily totals and top spenders; catch_up, rebuild, as_of | incremental catch-up at random points equals rebuild; as_of(p) equals a manual replay of the first p events; ties in top spenders order by name | needs: none | 667-753, 1022-1026 |
| m10 | Transfer reactor | 60 | core | process manager with its own checkpoint, deterministic receive command id, injected duplicate delivery and lost checkpoint | with seeded duplicates and forced checkpoint loss each transfer is received exactly once; money in flight returns to zero after draining | needs: none | 755-800 |
| m11 | Outbox relay and deduplicating consumer | 45 | core | pending positions, ack loss and redelivery, consumer dedupe by event id | every publishable event is applied exactly once while received count may be higher; pending drains | needs: none | 802-859 |
| m12 | Deterministic multi-writer simulation | 90 | core | writers that load and commit in separate scheduler steps, seeded command generator, conflict retry with the same id, background reactor, projections and relay, quiesce | same seed gives an identical log, different seed differs; conflicts, rejections and duplicates all occur | needs: none | 42-61, 861-1020 |
| m13 | System invariants across seeds | 60 | core | money conservation, gapless positions and versions, replay determinism, projections equal aggregates, FNV log fingerprint | all invariants over several seeds with small step counts | needs: none (cookbook run sizes are too big for a 15 s debug cap; reduce them) | 74-79, 1028-1131, 1437-1448 |
| m14 | End-to-end crash comparison and demos | 45 | optional | memory and file stores produce the same log, crash then reproject and continue, scripted demos and report | fingerprints equal; recovered log is a prefix; balances after reprojection equal aggregates | needs: none | 1133-1454 |

**Readiness**

| Skill, concept or API | Taught by | Status | First needed |
|---|---|---|---|
| command side as pure decide(state, command) and total evolve(state, event) with state as a fold of events | decide-evolve (LIN 10) *prep* | to be written | m1 |
| typed domain errors with thiserror, transparent wrapping and From | error-layers (ERR 2) | staged draft: `l8-import-errors`, `l8-fix-lost-context` | m1 |
| correction by compensating event instead of deletion | saga (RESIL 8) | to be written | m2 |
| invariants that conserve money across accounts, including in-flight amounts | bank-ledger (PAT 6) | to be written | m2 |
| serde adjacently tagged enum and derive for stored JSON | serde-wire (BIN 3) | staged draft: `f7-wire-format-json` | m3 |
| append-only store with per-stream expected version, global positions and optimistic concurrency | lin-event-store (LIN 4) | to be written | m3 |
| idempotent command ids | idempotency-store (RESIL 4) | to be written | m4 |
| retry after version conflict and replay to a past version | lin-event-store (LIN 4) | to be written | m4 |
| snapshot as a cache over the log (snapshot plus tail equals full replay) | decide-evolve (LIN 10) *prep* | to be written | m5 |
| upcasting old events on read through serde_json::Value, to_value and from_value | lin-upcasting (LIN 6) | to be written | m6 |
| CRC-32 | checksums (BIN 12) *prep* | to be written | m7 |
| length-prefixed checksummed records, torn-tail detection | wal (STORE 2) | exists: `y9-kv-store-wal` | m7 |
| OpenOptions append, write before apply, sync_data, group commit, set_len truncation | wal (STORE 2) | exists: `y9-kv-store-wal` | m8 |
| projections with checkpoint, rebuild and as-of replay | lin-projections (LIN 5) | to be written | m9 |
| relay from the committed log with at-least-once delivery, deterministic ids and inbox dedupe (reactor and outbox) | outbox (RESIL 7) | to be written | m10 |
| seeded xorshift with below(n) and percentage chance | seeded-rng (TEST 8) *prep* | to be written | m10 |
| deterministic single-thread simulation with seeded scheduling and fault injection | sim-network (DIST 1) | to be written | m12 |
| check at commit that state read earlier is still current | occ (TXN 5) | to be written | m12 |
| random-operation invariant checking | write-tests (TEST 2) | staged draft: `y5-catch-the-broken-queue` | m13 |
| stable FNV-1a hashing for a log fingerprint | feature-flags (PAT 8) | to be written | m13 |
| differential testing of two store implementations | test-differential (TEST 6) | to be written | m14 |

<details><summary>Suspected bugs and weak spots in the Cookbook file (9 notes, all unverified: found by reading, nothing run)</summary>

- lines 465-486 and 505-507: crc32 of an empty payload is 0, so a zero-filled tail (8 zero bytes) scans as a valid empty frame; serde_json::from_slice on it fails and FileStore::open errors forever instead of truncating (unverified)
- lines 503-514: open() truncates at the first bad checksum, so one flipped byte mid-log permanently deletes every later acknowledged batch; the demo's flip at byte 20 would empty the whole file (unverified)
- lines 544-548: if write_all partly fails, or sync_data fails after the write, memory is not committed but bytes stay in the file; later appends follow garbage (lost on recovery) or reuse the same pos and version (duplicates on reopen) (unverified)
- lines 640-644: a domain rejection computed from a stale loaded state is returned without any version check, so a command can be refused although current state would allow it (visible in the interleaved sim) (unverified)
- lines 637-638: duplicate detection compares only the command id, so a different command reusing an id reports success; rejected commands are not remembered (unverified)
- lines 709-718: reversal adjusts spent and the daily totals on the day of the Reversed event, not of the original, so a day's deposited or withdrawn can become negative (unverified)
- lines 156-163, 606 and 646: cmds and deltas grow without bound and are cloned on every load and snapshot (quadratic cost) (unverified)
- lines 515 and 497-524: no directory fsync after creating the log file (unverified)
- lines 1000 and 1044: check_money takes an unused balances argument, so the mid-run call checks nothing about the read model

</details>

#### `cap-jobsvc`: Job service over axum

Web services · medium · P2 · Cookbook: exact `job-service-axum`. Existing practice: b1-api-error-response (draft), c4 drafts. Today: partly: axum and tower oneshot are vendored; needs several modules in one lib.rs; no sockets needed.
Place: after roadmap position 293 of 435, `iv-http-range` (WEB 9), in the branch order. Series ending here: ERR, WEB.

**Whole file read: yes.**

Lines read: `job_service_axum.rs` 993 of 993.

**Milestones**

| # | Milestone | Min | Core? | Adds | Tested by | Runner lacks | Cookbook lines |
|---|---|---|---|---|---|---|---|
| m1 | API error shape and job spec validation | 45 | core | ApiError to status, stable code and details; Spec with serde defaults and deny_unknown_fields; validation collects every problem | table of bad specs gives all problems at once; each error variant renders the expected status and JSON | needs: axum, serde, serde_json, thiserror | 55-156 |
| m2 | Job store and idempotent submit | 60 | core | Status, per-job event log through one record function, JSON view, logical clock, key map and insert under one lock (no workers yet) | same key and body returns same job; same key different body is a conflict; racing submits from threads create one job; event ids are 1,2,3 | needs: none | 158-235, 239-253, 263-325 |
| m3 | Create and read endpoints | 60 | core | router, State, HeaderMap and Bytes extractors, 201 with Location, 200 replay header, GET by id | drive Router with tower oneshot; missing key, junk body, invalid spec and unknown or non-numeric id; invalid requests leave no job | needs: no sockets; read bodies with axum::body::to_bytes because http_body_util is not vendored | 446-451, 478-517, 595-658, 678-683 |
| m4 | Cursor pagination | 45 | core | after and limit query, BTreeMap range, one extra row to find next_after, clamped limit | walking pages over 7 jobs returns all ids once in order, including inserts between pages; bad query gives bad_query; after = u64::MAX returns an empty page | needs: none | 519-549 |
| m5 | Background workers with a concurrency limit | 90 | core | one task per job in a JoinSet with reaping, semaphore permit, running then succeeded or failed, simulated resize | tokio test with paused time: jobs reach expected terminal states with outputs and errors; all permits return | needs: tokio test-util (vendored) | 327-354, 390-400, 408-409, 431-444, 685-693 |
| m6 | Panic isolation and in-flight guard | 45 | core | run the work in a child task, turn JoinError panic payload into failed, RAII in-flight counter with high-water mark | a panicking job fails with its message and later jobs still succeed; 8 jobs with limit 2 never exceed a peak of 2 and the count returns to 0 | needs: none | 245-261, 356-360, 401-405, 410, 420-429 |
| m7 | Cancellation of queued and running jobs | 90 | core | watch channel per job, select around permit wait and around work, abort the child, cancel endpoint returning 202 or conflict | paused time: cancel a queued job (never started in its log) and a running one; second cancel and cancel of a finished job conflict; permit is released | needs: none | 305, 382-397, 406, 412-416, 551-564 |
| m8 | Server-sent events replay | 60 | core | events endpoint that replays the job log after Last-Event-ID with SSE ids, names and keep-alive | collect the body: content type, ids in order, resume from id 2 gives only id 3, unknown job is 404 | needs: use futures::stream because futures_util is not a direct dependency | 566-591 |
| m9 | Graceful shutdown | 60 | core | stop accepting with 503, drain the JoinSet, report outcome counts, reads keep working | paused time: shutdown returns only after queued and running jobs finish; report counts; late POST gets shutting_down; GET still 200 | needs: none | 228-235, 362-383, 974-991 |
| m10 | Request id middleware and tracing | 60 | core | from_fn layer that echoes a sane x-request-id or generates one, span per request, Instrument | valid id echoed; empty, over-long or non-graphic id replaced by a generated one; header present on error responses | needs: tracing, tracing-subscriber | 453-476 |
| m11 | Scenario client and transcript | 45 | optional | helper client, polling helpers, scripted main with log subscriber and panic hook | script runs through all scenarios in-process | needs: none | 593-693, 695-993 |

**Readiness**

| Skill, concept or API | Taught by | Status | First needed |
|---|---|---|---|
| serde defaults, deny_unknown_fields, lowercase rename | serde-wire (BIN 3) | staged draft: `f7-wire-format-json` | m1 |
| axum IntoResponse for one JSON error shape with stable codes | axum-api (WEB 3) | staged draft: `b1-api-error-response` | m1 |
| job status as an enum state machine with a single transition function | vending-machine (PAT 1) | to be written | m2 |
| idempotency keys with body comparison, atomic check and insert | idempotency-store (RESIL 4) | to be written | m2 |
| Arc shared state with std Mutex, never holding a guard across await | send-across-await (ASYNC 4) | to be written | m2 |
| recovering a poisoned mutex (PoisonError::into_inner) | panic-boundary (ERR 3) | to be written (partly practised by `l1-rollback-guard`) | m2 |
| axum Router, State, Path, Query, HeaderMap and Bytes extractors, tuple responses with headers, driving with tower::ServiceExt::oneshot | axum-api (WEB 3) | staged draft: `b1-api-error-response` | m3 |
| cursor pagination over BTreeMap range with limit plus one | cursor-pagination (WEB 2) | to be written | m4 |
| tokio Semaphore with acquire_owned as a concurrency limit | fanout-limit (ASYNC 5) | staged draft: `c4-bounded-fan-out` | m5 |
| JoinSet spawn, try_join_next reaping and join_next draining | graceful-shutdown (ASYNC 7) | staged draft: `c4-drain-with-deadline` | m5 |
| child task for panic isolation: JoinHandle, JoinError::try_into_panic, payload downcast | tokio-job-outcome (ASYNC 13) *prep* | to be written | m6 |
| RAII guard that returns a count when dropped, including on abort | raii-pool (SMART 1) | to be written | m6 |
| atomic counters and high-water mark with fetch_add and fetch_max | atomic-counters (MON 1) | exists: `c3-atomic-stats` | m6 |
| per-job cancellation with watch (wait_for, send_replace), select! over permit or work, AbortHandle | tokio-job-outcome (ASYNC 13) *prep* | to be written | m7 |
| SSE with axum Sse, Event, KeepAlive, impl Stream return type and Last-Event-ID replay | sse-replay (WEB 7) *prep* | to be written | m8 |
| stop accepting, then drain and report | graceful-shutdown (ASYNC 7) | staged draft: `c4-drain-with-deadline` | m9 |
| axum middleware::from_fn, header validation, tracing spans, Instrument and in_scope | axum-request-tracing (WEB 8) *prep* | to be written | m10 |

<details><summary>Suspected bugs and weak spots in the Cookbook file (8 notes, all unverified: found by reading, nothing run)</summary>

- lines 290-324 and 363-367: submit checks accepting before locking and spawns after unlocking; a concurrent shutdown can take the JoinSet in between, so the new task lands in a fresh set that is never joined and the report shows it unfinished (unverified)
- line 535: after + 1 overflows for ?after=18446744073709551615; panics in debug while the store lock is held, wraps in release and returns the first page (unverified)
- lines 365-367: a second concurrent shutdown call takes an empty JoinSet and returns before jobs finish (unverified)
- lines 412-414: abort() is not awaited, so the job is marked cancelled and its permit freed while the child may still be running; the concurrency bound can be briefly exceeded (unverified)
- lines 392-395 and 406-416: select! is not biased; a cancel acknowledged with 202 may still end succeeded, and an already cancelled queued job may take a permit and log job.started first (unverified)
- lines 577-590: SSE only replays a snapshot and ends; there is no live tail although the header says clients stream the event log, and KeepAlive has no effect (unverified)
- lines 221-226 and 291-301: idempotency keys never expire, have no length limit and are not scoped per client; replay of an existing key gets 503 during shutdown (unverified)
- lines 35-36: imports futures_util and http_body_util, which are not in docker/deps/Cargo.toml, so the file cannot be used as is under the runner

</details>

#### `cap-lease`: Lease and fencing simulator

Distributed · medium · P2 · Cookbook: exact `lease-fencing-sim`. Existing practice: y7 (rules only). Today: yes.
Place: after roadmap position 392 of 435, `iv-sim-invariant-search` (TEST 12), in the branch order. Series ending here: TEST.

**Whole file read: yes.**

Lines read: `lease_fencing_sim.rs` 519 of 519.

**Milestones**

| # | Milestone | Min | Core? | Adds | Tested by | Runner lacks | Cookbook lines |
|---|---|---|---|---|---|---|---|
| m1 | Seeded RNG and deterministic event loop | 60 | core | xorshift with seeding, event enum, min-heap on (time, seq), schedule, latency, step limit, optional trace | same seed gives identical trace; equal times pop in scheduling order; step cap trips on a self-rescheduling event | needs: none | 30-80, 211-264, 277-290 |
| m2 | Lease service with fencing tokens | 45 | core | TTL grant, strictly increasing tokens, renew only for the live holder | boundary cases at expiry; old holder renewal refused | needs: none | 82-128, 411-436 |
| m3 | Fenced storage and safety checker | 45 | core | optional token check, accepted and arrived histories, two safety invariants | hand-written histories that pass and violate each invariant | needs: none | 130-193 |
| m4 | Client protocol under pauses | 90 | core | acquire, write, ack, heartbeat, backoff and pause handling as event transitions; ack verdict does not affect scheduling | zero pause percent gives no violations and all leases finish; same seed same trace; identical arrived writes with and without fencing | needs: none | 195-209, 265-276, 292-398 |
| m5 | Seed search, paired replay and campaign | 60 | core | find first corrupting seed, replay with fencing, 2000 paired schedules | fenced violations zero, unfenced above zero, stale rejections above zero | needs: none; check 4000 runs fit the 15 s debug cap or lower the count | 438-443, 448-457, 465-518 |
| m6 | Timeline printing and narrated scenarios | 45 | optional | filtered trace output for scenarios A and B | filtered lines contain the pause and the rejected write for the fixed seed | needs: none | 400-409, 458-464, 478-481 |

**Readiness**

| Skill, concept or API | Taught by | Status | First needed |
|---|---|---|---|
| seeded event queue with (time, sequence) ordering via BinaryHeap and Reverse | sim-network (DIST 1) | to be written | m1 |
| seeded xorshift RNG with ranges and percentages | sim-network (DIST 1) | to be written | m1 |
| lease with TTL, renewal and monotonic fencing token | lease-fencing (DIST 7) | to be written | m2 |
| resource-side rejection of stale tokens | lease-fencing (DIST 7) | to be written | m3 |
| client protocol as an event-driven state machine | sans-io (NET 1) | to be written | m4 |
| safety invariants over a recorded history, seed search for a violation and mode-independent schedules for paired replay | sim-invariant-search (TEST 12) *prep* | to be written | m3 |

<details><summary>Suspected bugs and weak spots in the Cookbook file (3 notes, all unverified: found by reading, nothing run)</summary>

- the comment at lines 316-317 says the client checks lease validity at grant time, but there is no client-side check: the debug_assert is vacuous whenever pause_percent > 0, expiry is otherwise unused by the client, and writes are never gated on it. The sim therefore does not demonstrate that check-then-write fails (unverified as intent).
- model weak spots: no message loss, duplication or reordering of a single client's traffic, pauses happen only before a write is sent, and the lease service never restarts (token persistence untested). Fencing only rejects a stale token after storage has seen a newer one, so an expired holder's write that arrives first is accepted; worth stating to the learner.
- no correctness bug found in the lease service, storage or checker by reading (unverified by execution).

</details>

#### `cap-runtime`: Mini async runtime

Async · hard · P1 · Cookbook: exact `mini-async-runtime`. Existing practice: c5-block-on, c5-mini-executor, c5-oneshot-channel. Today: stages 1 to 5 yes; reactor stage needs mio (not vendored); work stealing can use crossbeam.
Place: after roadmap position 283 of 435, `iv-mio-reactor-api` (NET 10), in the branch order. Series ending here: SMART, ASYNC.

**Whole file read: yes.**

Lines read: `mini_async_runtime.rs` 2516 of 2516.

**Milestones**

| # | Milestone | Min | Core? | Adds | Tested by | Runner lacks | Cookbook lines |
|---|---|---|---|---|---|---|---|
| m1 | block_on with a parking waker, and yield_now | 45 | core | ParkWake over thread park/unpark with a flag, pinned root future, yield_now (the thread-local enter call is added in m4) | hand-written futures that wake themselves or are woken from a joined thread; count polls, check output; yield_now returns Pending exactly once and wakes first | needs: none | 357-389, 1005-1019 |
| m2 | Task and its wake state machine | 90 | core | Task holding Mutex of Option boxed future, Scheduler trait, IDLE/SCHEDULED/RUNNING/NOTIFIED/COMPLETE, notify and run, Wake for Arc of Task, exclusivity flag and live-task counter | recording scheduler that only collects tasks and is stepped by hand: wake twice enqueues once, wake during poll re-enqueues once, wake after completion does nothing, live count returns to zero | needs: none | 64-67, 86-201 |
| m3 | JoinHandle, panic capture, cancellation and abort | 90 | core | JoinInner slot, Harness with catch_unwind per poll, Cancelled delivered on drop, JoinHandle future, abort, is_finished, spawn_on | hand-stepped scheduler: output delivered; panic message in JoinError; abort gives Cancelled and a drop guard has run by the time the result is visible; dropping the handle detaches; abort after completion keeps the result | needs: none | 203-316 |
| m4 | Runtime handle and thread-local context | 45 | core | Handle, CURRENT thread local, restoring EnterGuard, current and free spawn; enter added to block_on | nested enter restores the previous handle, also across a caught panic; spawn outside a runtime panics; spawn from inside block_on reaches the right queue | needs: none | 318-356 |
| m5 | Single-threaded executor | 60 | core | LocalQueue as Scheduler, root waker with flag and condvar, run_until_stalled, block_on that interleaves root and tasks without timers | three yielding tasks log exactly 0 1 2 repeated; root woken from a joined external thread completes; run_until_stalled returns the number of polls | needs: none | 391-517 |
| m6 | Injectable clock, timer heap and Sleep | 90 | core | real or manual Clock, reversed Ord entry with sequence number, insert, fire_due, next_deadline, Sleep with lazy cancellation, wired into the single-threaded executor | manual clock: 1000 permuted deadlines fire in order, none early, exactly those with deadline at or before now; equal deadlines FIFO; dropped sleeps never wake | needs: none | 751-944, 448-450, 483, 501-514 |
| m7 | timeout and interval | 45 | core | Timeout that polls the value first then the deadline and drops the loser; Interval with immediate first tick and burst catch-up | manual clock: not expired at 49, expired at 50, value ready at the deadline wins, inner drop guard ran once; ticks counted 1 then 4 after advancing 35 | needs: none | 946-1000 |
| m8 | Oneshot channel | 45 | core | sender and receiver halves with gone flags, wake on sender drop, value returned if receiver gone | on the single-threaded executor: value before and after first poll, sender dropped gives error and wakes, send after receiver drop returns the value | needs: none | 1021-1081 |
| m9 | Bounded async mpsc | 90 | core | cloneable sender with waker backpressure, sender counting, receiver close wakes all senders, cancel-safe send and recv, max-length probe | deterministic executor: capacity never exceeded, per-producer order kept, last sender drop ends the stream, send to closed returns value, dropped pending send enqueues nothing and does not strand other senders | needs: none | 1083-1195 |
| m10 | Semaphore and Notify | 60 | core | permit guard released on drop, wake-all waiter list with will_wake dedup, Notify with one stored permit | max concurrent holders never exceeds n; notify before wait is kept; two notifies before wait give one permit; cancelled waiter does not block another | needs: none | 1197-1265 |
| m11 | join2, join_all and select2 | 90 | core | MaybeDone, polling every child, result order, biased select that drops the loser at once | hand-polled children with counters: both polled even if one is done, results in input order, tie goes to the left, loser destructor runs before select returns, empty join_all is ready | needs: none | 1267-1368 |
| m12 | Timer driver thread | 45 | core | driver loop that sleeps to the next deadline, is re-armed by an earlier insert, fires outside the lock and stops on request | many short real sleeps all complete and none early (lower bound only); an earlier sleep inserted after a very long one completes; stop joins the thread | needs: none | 806-831, 858-880 |
| m13 | Work-stealing queues and task finding | 90 | core | Injector, per-worker FIFO deque and stealer, worker thread-local context with runtime id, own-deque fast path, periodic global-first check, steal retry loop, executed and stolen counters | drive one worker context by hand on one thread: outside schedules land in injector, inside schedules in local deque, every 61st tick takes injector first, a foreign runtime id uses the injector | needs: none | 519-609 |
| m14 | Worker sleep and wake, Runtime start and graceful drop | 120 | core | sleepers count with fence and recheck under lock, wake_one, worker_main, Runtime new and block_on, Drop that stops and joins all threads and discards queued tasks, thread counting helper | 100000 tiny tasks each run once and polls equal 100000; external threads hammering wake never cause a concurrent poll; thread count returns to baseline after drop (outer test cap catches a hang) | needs: none | 68-84, 611-746 |
| m15 | Edge-triggered readiness to waker bridge | 60 | core | IoState with read and write epochs and waker slots, poll_io: remember epoch, try operation, publish waker, recheck epoch | fake operation closure that returns WouldBlock and fires an event in between: must retry not sleep; Interrupted retried; read and write wakers independent | needs: none | 1374-1380, 1441-1452, 1468-1499 |
| m16 | mio reactor thread | 90 | core | Reactor with Poll, cloned Registry, token map, mio Waker for shutdown, Io wrapper that deregisters on drop, added to Runtime and Handle | register a loopback source, make it ready from a std thread, stored waker fires; drop removes the token; shutdown joins the reactor | needs: mio is not in the runner; loopback sockets unverified | 1370-1467, 670-693, 736-737 |
| m17 | Async TcpListener and TcpStream | 90 | core | bind, accept, connect with take_error and peer_addr probe, read, read_exact, write_all, half close, cloneable stream | loopback echo of a payload larger than socket buffers with reader and writer joined on one task; refused connect returns an error; early EOF from read_exact | needs: mio is not in the runner; loopback sockets unverified | 1501-1582 |
| m18 | Stress and hazard suite | 60 | optional | gate future hammered by external threads, work-spreading demo, real-clock timer batch, panic storm with surviving workers, report table | completion, exactly-once counters, zero poll violations, zero live tasks and threads; steal counts printed and not asserted | needs: none | 1584-2011, 2438-2465, 2467-2516 |
| m19 | Many-client echo scenario | 45 | optional | echo_conn and echo_client with 20 concurrent clients and reactor event counter | bytes echoed equal bytes sent for each client | needs: mio is not in the runner; loopback sockets unverified | 2127-2202 |
| m20 | Differential check against tokio | 60 | optional | the same fan-out, pipeline, time, panic and echo programs on tokio and on this runtime | results compared for equality | needs: mio and loopback only for the echo program; the other four run with tokio | 2204-2436 |

**Readiness**

| Skill, concept or API | Taught by | Status | First needed |
|---|---|---|---|
| custom wakers with std::task::Wake on an Arc, Context::from_waker | block-on (ASYNC 1) | exists: `c5-block-on` | m1 |
| thread park and unpark with a flag against spurious wakeups | block-on (ASYNC 1) | exists: `c5-block-on` | m1 |
| pinning a root future with pin! and Box::pin, hand-written Future::poll | block-on (ASYNC 1) | exists: `c5-block-on` | m1 |
| tasks as boxed Send futures that re-queue themselves through their wakers | executor (ASYNC 2) | exists: `c5-mini-executor` | m2 |
| task wake state machine: schedule at most once, remember a wake during poll | task-wake-state (ASYNC 15) *prep* | to be written | m2 |
| compare_exchange_weak loop over an atomic state | once (ATOM 2) | exists: `c3-once-cell` | m2 |
| Acquire, Release and AcqRel orderings | spinlock (ATOM 1) | exists: `c3-spinlock` | m2 |
| completion slot that stores the joiner's waker (JoinHandle) | async-channel (ASYNC 3) | to be written (partly practised by `c5-oneshot-channel`) | m3 |
| catch_unwind, AssertUnwindSafe and panic payload downcast | panic-boundary (ERR 3) | to be written (partly practised by `l1-rollback-guard`) | m3 |
| cancellation is dropping the future; which futures lose data when dropped | cancel-safety (ASYNC 6) | staged draft: `c4-fix-cancel-safe-reader` | m3 |
| thread-local current runtime installed by a restoring guard | ambient-context (SMART 4) *prep* | to be written | m4 |
| Condvar wait in a loop, taking the lock before notify so a check cannot miss it (parking_lot Mutex and Condvar, wait_for) | blocking-queue (CONC 2) | exists: `c1-blocking-queue` | m5 |
| injected real or manual clock | fake-clock (TEST 1) | to be written | m6 |
| BinaryHeap as time-ordered queue: reversed Ord, FIFO tie-break, lazy cancellation, next deadline | job-scheduler (CONC 15) | to be written | m6 |
| register the waker then recheck a lock-free flag or epoch | register-recheck (ASYNC 17) *prep* | to be written | m6 |
| async timeout: race a future against a timer and drop the loser | async-primitives (ASYNC 10) | to be written | m7 |
| std::future::poll_fn and storing a waker where the event happens | async-channel (ASYNC 3) | to be written (partly practised by `c5-oneshot-channel`) | m8 |
| disconnection protocol: sender counts and closed receiver | channel-mpsc (CONC 7) | exists: `c2-bounded-channel` | m8 |
| bounded async channel with waker backpressure | async-channel (ASYNC 3) | to be written (partly practised by `c5-oneshot-channel`) | m9 |
| async semaphore with permit guard and notify with stored permit; cancelled waiters; will_wake dedup | async-primitives (ASYNC 10) | to be written | m10 |
| hand-written join and select combinators over pinned boxed children | future-combinators (ASYNC 16) *prep* | to be written | m11 |
| driver thread sleeping to the next deadline and re-armed on earlier insert | job-scheduler (CONC 15) | to be written | m12 |
| per-worker deques with stealing (crossbeam::deque Injector, Worker, Stealer, Steal::Retry, steal_batch_and_pop) | work-stealing (CONC 19) | to be written | m13 |
| idle-worker sleep handshake: sleeper count, SeqCst fence, recheck under lock | park-wake-handshake (CONC 22) *prep* | to be written | m14 |
| per-worker statistics with atomics | atomic-counters (MON 1) | exists: `c3-atomic-stats` | m13 |
| joining a fixed set of worker threads and draining on shutdown | thread-pool (CONC 9) | exists: `c2-thread-pool` | m14 |
| edge-triggered readiness bridged to wakers with an epoch counter | register-recheck (ASYNC 17) *prep* | to be written | m15 |
| readiness loop: nonblocking operations, WouldBlock, token to source table | event-loop (NET 8) | to be written | m15 |
| mio API: Poll, Events, Registry, Token, Interest, mio::Waker, event::Source, nonblocking connect | mio-reactor-api (NET 10) *prep* | to be written | m16 |
| real sockets: loopback bind on port 0, half close, short reads and writes | tcp-echo-framed (NET 5) | to be written | m17 |
| differential testing against a reference implementation (tokio runtime builder, spawn, mpsc, time, select) | test-differential (TEST 6) | to be written | m20 |

<details><summary>Suspected bugs and weak spots in the Cookbook file (13 notes, all unverified: found by reading, nothing run)</summary>

- unverified: Harness::drop (256-259) runs before its fields are dropped, so on abort the joiner is woken with Cancelled before the user future's destructors run; the asserts at 1991-1997 (drop count 1, tx.send fails) can race on the multi-thread runtime
- unverified: on Ready or panic, join.complete is called (246, 250) before the user future is dropped (173-175), so a joiner can observe completion while its resources are still held
- unverified: catch_unwind covers only poll; a panicking destructor during abort or completion (155, 175) unwinds and kills the worker thread
- unverified: Drop for Runtime says dropping never-run tasks drops their futures (741-743), but a live JoinHandle holds another Arc of the task, so the future survives and the handle never resolves; spawn after shutdown also hangs
- unverified: IoState has one waker slot per direction while TcpStream is Clone and accept takes &self; two tasks waiting in the same direction overwrite each other and one is never woken
- unverified: leak cycle beyond the header note: reactor sources to IoState to waker to task to future to Io to Arc of Reactor; tasks parked on IO at shutdown are never freed
- unverified: cancelled sleeps stay in the heap until their deadline (929-933), so repeated long timeouts grow memory and still wake the driver
- unverified: with a manual clock, advance does not notify the timer driver or the local condvar; block_on then waits that many real nanoseconds (501-511, 871-875)
- unverified: the 50 ms wait_for safety net (639) would hide a lost-wakeup bug in the sleep handshake, so the tests cannot detect one
- unverified: run stores RUNNING with a plain store and notify returns without a read-modify-write when it sees SCHEDULED; the lost-wakeup argument then relies on the event source's own synchronisation
- unverified: JoinHandle, Select2 polled after completion return Pending forever (or silently hang) instead of failing loudly
- unverified: Semaphore, Notify and mpsc wake all waiters (thundering herd, no fairness), unlike tokio named in the header; stale wakers of cancelled waiters stay until the next wake
- unverified: test weak spot: t_work_stealing (1673-1682) asserts that stealing happened, which depends on OS scheduling

</details>

#### `cap-httpserver`: HTTP server on a thread pool

Networking · hard · P1 · Cookbook: exact `mini-http-server-thread-pool`. Existing practice: b4-http1-request-parser. Today: parser, router, files-in-memory stages yes; server stages need loopback in the sandbox (not verified); tokio comparison optional.
Place: after roadmap position 420 of 435, `iv-log-linear-histogram` (MON 9), in the branch order. Series ending here: TRAIT, NET.

**Whole file read: yes.**

Lines read: `mini_http_server_thread_pool.rs` 2398 of 2398.

**Milestones**

| # | Milestone | Min | Core? | Adds | Tested by | Runner lacks | Cookbook lines |
|---|---|---|---|---|---|---|---|
| m1 | Request line and header parsing with limits | 60 | core | HttpError to status, Limits, Request, Parsed Incomplete or Done or Bad, CRLF search, request line and header validation, Host rule | table of inputs; every prefix of a valid request is never Bad; limit cases give 414 and 431; random bytes never panic | needs: none | 130-271 |
| m2 | Body framing, smuggling checks and keep-alive decision | 60 | core | Content-Length agreement and digits, Transfer-Encoding checks, early 413, bytes used for pipelining, Expect flag, wants_keep_alive | pipelined buffer parsed twice by used count; CL plus TE and conflicting CL rejected; oversized length rejected before body arrives | needs: none | 272-329, 380-387 |
| m3 | Chunked request decoding | 60 | core | size lines with extensions, trailers, size limit, CRLF check, incomplete handling | every split point of a chunked request gives the same final result; oversize chunk gives 413; bad hex and missing CRLF give 400 | needs: none | 331-378 |
| m4 | Response model and serialiser | 60 | core | Response builder, Empty or Bytes or Stream body, reason phrases, Content-Length, chunked streaming with flush, HEAD and bodyless statuses, error_response | write into a Vec and compare exact bytes; HEAD has same headers and no body; empty stream chunks skipped; returned byte count equals bytes written | needs: none | 406-531, 1100-1105 |
| m5 | Router with path parameters | 60 | core | literal, parameter and rest segments, registration order, HEAD served by GET, 404 and 405 with Allow | table of method and path pairs against expected status, parameters and Allow | needs: none | 533-633 |
| m6 | Middleware chain, request id, access log, panic to 500 | 60 | core | Handler and Middleware types, recursive run_chain, App (its metrics field and panic counter arrive in m8), request id validation, bounded log ring, catch_panic, basic routes | call App handle directly: order of wrappers visible, bad ids replaced, log capped at 64, panicking route gives 500 | needs: none | 547-549, 635-652, 985-1015, 1026, 1039-1070, 1082-1098 |
| m7 | Log-linear latency histogram | 45 | core | bucket index and upper bound, atomic record, quantile, cumulative_le | index and upper round trip for all boundaries; quantiles of 1 to 10000 within 12.5 percent; empty histogram returns 0 | needs: none | 57-128 |
| m8 | Metrics registry, Prometheus text and timing middleware | 60 | core | counters and gauges, status classes, text exposition with cumulative buckets, metric_value reader, timing middleware, /metrics route | fixed counter values rendered and parsed back; cumulative buckets are non-decreasing; +Inf equals count; Server-Timing header present (value not asserted) | needs: none | 654-785, 1020-1025, 1071-1081 |
| m9 | Static files with traversal protection | 60 | core | percent decoding, segment filtering, canonicalize and prefix check, content type, file response | temp directory with a symlink that escapes the root; list of hostile paths gives 403 or 404 and never the secret; nested file served | needs: none (unix symlink in tmpfs) | 389-404, 863-902, 941-944, 1016-1019 |
| m10 | Validators and conditional GET | 45 | core | HTTP date from SystemTime, ETag from length and mtime, Last-Modified, If-None-Match precedence and 304 | known timestamps formatted exactly; matching, weak, star and stale tags; If-Modified-Since equality | needs: none | 787-819, 903-930 |
| m11 | Byte ranges | 45 | core | single-range parser with suffix, open end, clamping, ignored forms, 206 and 416 with Content-Range | table over lengths including 0; served bytes equal the file slice | needs: none | 821-861, 931-940 |
| m12 | Serve one connection: keep-alive, pipelining, 100-continue, errors | 90 | core | read, parse, handle, write loop, drain used bytes, max requests per connection, single interim 100, error then lingering close, active gauge guard, connection config | accept one loopback connection in a test thread: five requests in one write answered in order, byte-at-a-time request works, Nth response says close, each protocol error gets its status then EOF | needs: loopback sockets unverified | 1109-1141, 1325-1398, 1401-1404, 1426-1427 |
| m13 | Idle and request timeouts | 60 | core | 50 ms read slices, idle close counted, 408 for a stalled partial request, stop flag noticed while idle | small timeouts: half a request then silence gives 408 then EOF; idle connection reaches EOF and counter rises; assert outcomes and not durations | needs: loopback sockets unverified | 1399-1425 |
| m14 | Acceptor, bounded queue, fixed pool and load shedding | 90 | core | sync_channel with try_send, shared receiver workers, 503 with Retry-After, queued and rejected metrics, per-connection panic guard, gate route to saturate the pool | 2 workers and queue 2 blocked on a gate: wait on counters, fifth connection gets 503, all admitted work completes after the gate opens; panic with one worker leaves it serving | needs: loopback sockets unverified | 949-983, 1027-1032, 1143-1216, 1239-1292, 1299-1323 |
| m15 | Graceful shutdown | 60 | core | stopping flag, self-connect to wake accept, drop sender to drain, join all, live-thread guard and report, in-flight answers with Connection close | slow handler signals start through a channel, shutdown is called, response arrives with close, idle connection gets EOF, live thread count is zero, new connect refused | needs: loopback sockets unverified | 1033-1037, 1150-1176, 1217-1237, 1293-1297 |
| m16 | Thread-per-connection mode | 45 | optional | Mode switch, thread per socket, handle reaping, join on shutdown | same request suite passes in both modes; live count zero after shutdown | needs: loopback sockets unverified | 1275-1290 |
| m17 | Blocking test client | 90 | optional | persistent client with leftover buffer, response parsing for length and chunked bodies, HEAD, EOF detection | feed canned responses from a loopback peer in small pieces | needs: loopback sockets unverified | 1492-1622 |
| m18 | End-to-end scenario run | 90 | optional | temp site fixture, panic hook, the twelve scripted sections | scripted assertions; drop the 150 ms lower bound on idle close or inject the clock | needs: loopback sockets unverified; builds on m17 | 1682-2312, 2396-2398 |
| m19 | Load generator and report | 60 | optional | barrier-started client threads, per-request latency into the histogram, report table | every response body matches its unique request and histogram count equals total; rates printed only | needs: loopback sockets unverified; builds on m17 | 1624-1680, 2312-2352, 2355-2395 |
| m20 | Tokio comparison server on the same parser and router | 90 | optional | tokio listener, per-connection task, shared parser, router and writer into a Vec, read timeout, runtime shutdown | same request cases as the thread server give the same status and body | needs: loopback sockets unverified | 1429-1488, 2346-2354 |

**Readiness**

| Skill, concept or API | Taught by | Status | First needed |
|---|---|---|---|
| incremental HTTP/1.1 request parsing as a pure function with limits | http-request (NET 6) | exists: `b4-http1-request-parser` | m1 |
| request smuggling rules: Content-Length against Transfer-Encoding, early rejection | http-request (NET 6) | exists: `b4-http1-request-parser` | m2 |
| chunked request body decoding with extensions and trailers | http-request (NET 6) | exists: `b4-http1-request-parser` | m3 |
| response framing: Content-Length, chunked streaming, HEAD, bodyless statuses, Connection header, interim 100 Continue, reading it back on a client | http-response-framing (NET 11) *prep* | to be written | m4 |
| route matching with path parameters, rest segments, 405 and Allow | router (WEB 1) | to be written | m5 |
| middleware chain of boxed closures with a borrowed next | fn-pipeline (ITER 2) | to be written | m6 |
| catch_unwind around a handler and a second boundary around the connection | panic-boundary (ERR 3) | to be written (partly practised by `l1-rollback-guard`) | m6 |
| bounded ring of recent log lines | mon-flight-recorder (MON 3) | to be written | m6 |
| log-linear histogram bucketing with leading_zeros and quantiles | log-linear-histogram (MON 9) *prep* | to be written | m7 |
| lock-free counters and fetch_max | atomic-counters (MON 1) | exists: `c3-atomic-stats` | m7 |
| Prometheus text exposition with cumulative histogram buckets | mon-metrics (MON 2) | to be written (partly practised by `c3-atomic-stats`) | m8 |
| percent decoding | base64 (TEXT 3) | to be written | m9 |
| safe join of an untrusted path, canonicalize and symlink escape | sec-safe-path (SEC 1) | to be written | m9 |
| civil date from days since epoch and HTTP date format | datetime-parse (PARSE 9) | to be written | m10 |
| ETag, If-None-Match, If-Modified-Since and 304 | http-cache (WEB 5) | to be written | m10 |
| byte Range requests, 206, 416 and Content-Range | http-range (WEB 9) *prep* | to be written | m11 |
| std TCP with read and write timeouts, WouldBlock against TimedOut, nodelay, accepted-socket blocking mode, time-sliced reads | tcp-echo-framed (NET 5) | to be written | m12 |
| buffer until a whole message arrives and keep the leftover for the next one | length-codec (BIN 2) | exists: `b4-length-prefixed-codec` | m12 |
| lingering close and why unread input causes RST | tcp-lingering-close (NET 12) *prep* | to be written | m12 |
| RAII guards for gauges and live-thread counts | raii-pool (SMART 1) | to be written | m12 |
| fixed worker pool over a shared channel receiver | thread-pool (CONC 9) | exists: `c2-thread-pool` | m14 |
| accept, bounded sync_channel with try_send, load shedding with 503, drain | http-server (NET 7) | to be written | m14 |
| condvar gate with wait_timeout_while to hold handlers | blocking-queue (CONC 2) | exists: `c1-blocking-queue` | m14 |
| waking a blocked accept by self-connect and joining every thread | tcp-echo-framed (NET 5) | to be written | m15 |
| start barrier for load-generator threads | barrier-latch (CONC 6) | to be written | m19 |
| latency measurement and percentile reporting | bench-harness (PERF 1) | to be written | m19 |
| tokio networking: runtime builder, TcpListener::from_std, AsyncRead and AsyncWrite extensions, time::timeout, task per connection | tokio-tcp-server (NET 13) *prep* | to be written | m20 |

<details><summary>Suspected bugs and weak spots in the Cookbook file (13 notes, all unverified: found by reading, nothing run)</summary>

- unverified: request_timeout is checked only in the read-timeout branch (1416), so a client sending one byte more often than every 50 ms never gets 408; it holds a worker until a size limit is hit and also delays graceful shutdown (bounded for headers, unbounded for chunked trailers)
- unverified: no bound on raw chunked bytes: trailers (354-364) and chunk extensions are not limited, only the decoded body is, so the connection buffer can grow without limit while data keeps coming (worse in tokio_conn, which has no request deadline)
- unverified: the 40-byte size-line limit applies only when CRLF has not arrived (339-344), so a long chunk extension is accepted or rejected depending on how TCP splits it
- unverified: only the first Transfer-Encoding header is examined (get at 263-268, 288), so a second differing one is silently accepted; duplicate Host headers are accepted too; Transfer-Encoding on HTTP/1.0 is accepted
- unverified: bare CR or LF and other control bytes are not rejected in the target or header values (256-261); str::trim also strips non-HTTP whitespace
- unverified: Body::Stream is sent chunked even to HTTP/1.0 clients (488-505), which cannot decode it
- unverified: pct_decode uses u8::from_str_radix, which accepts a leading plus sign, so %+5 decodes (395-396)
- unverified: parser rescans the buffer and re-decodes the whole chunked body after every read (quadratic, bounded only by the limits)
- unverified: write_response does not check header values for CR or LF; safety depends on every handler (response splitting risk)
- unverified: reject_503 runs on the acceptor thread and reads once without waiting (1300-1310), so a request arriving after close can still cause RST, and a slow peer can stall accepting for up to the write timeout
- unverified: static files are read whole into memory, and canonicalize then read leaves a check-to-use window
- unverified: render reads histogram buckets and count separately, so under load a finite bucket can exceed +Inf
- unverified: scenario 8 asserts a wall-clock lower bound (2169), a timing assertion that graded tests should not copy

</details>

#### `cap-kvstore`: LSM key-value store

Storage · hard · P1 · Cookbook: exact `mini-kv-store-code`. Existing practice: y9-kv-store-wal, f4-bloom-filter. Today: yes with an in-memory file trait; real files need a writable tmp (not verified).
Place: after roadmap position 390 of 435, `iv-crash-ordered-files` (STORE 20), in the branch order. Series ending here: STORE.

**Whole file read: yes.**

Lines read: `mini_kv_store.rs` 840 of 840.

**Milestones**

| # | Milestone | Min | Core? | Adds | Tested by | Runner lacks | Cookbook lines |
|---|---|---|---|---|---|---|---|
| m1 | Record codec and CRC32 | 60 | core | CRC32, bounds-checked take/take_u32/take_u64 with checked_add, tagged put/tombstone record encode and decode, KvError | CRC known vector; seeded round-trip; every truncation and bad tag returns Err, never panics | needs: crate thiserror (vendored) | 26-92 |
| m2 | Write-ahead log with torn-tail recovery | 90 | core | length+CRC frames, replay of the longest valid prefix, set_len truncation on open, append, reset | cut the log at every byte offset and compare with the expected prefix; appends after recovery are readable; flipped payload byte; zero-filled tail must open (the cookbook code is expected to fail this one) | needs: none (tmpfs /tmp) | 94-152 |
| m3 | Memtable store with replay | 60 | core | BTreeMap memtable with tombstones, log-then-apply put/delete, get, range scan, reopen replays the WAL | seeded ops against a BTreeMap with periodic reopen; scan with from equal to or greater than to returns empty | needs: none | 374-391, 440-463, 481-501 |
| m4 | Bloom filter | 45 | core | FNV-1a, double hashing, build, may_contain, encode and validated decode | no false negatives; false-positive rate under 5 percent on fixed key sets; malformed encodings rejected | needs: none | 154-208, 591-604 |
| m5 | SSTable file: write, open and point read | 120 | core | data, sparse index, bloom and footer layout; temp file, fsync, rename; footer, checksum and offset validation; get reads one index block; read_all | every written key found, absent keys (before, between, after) not found, tombstone distinct from absent; any flipped byte makes open fail; short file rejected | needs: none | 210-342 |
| m6 | Flush, layered reads and directory recovery | 90 | core | threshold flush then WAL reset, newest-to-oldest get with bloom skips and stats, overlay scan, open loads tables, removes tmp files, derives next id | model test with tiny threshold and reopen; leftover tmp removed; WAL restored over an already flushed table gives the same state | needs: none | 392-438, 464-519 |
| m7 | K-way merge | 60 | core | heap merge of sorted runs ordered newest first, shadowed versions discarded, optional tombstone drop | seeded runs against a BTreeMap overlay, both tombstone modes | needs: none | 344-372, 772-786 |
| m8 | Crash-safe full compaction | 60 | core | merge all tables, write new table with the highest id, delete old oldest first, handle empty result and delete failure without losing the in-memory table list | state equal before and after; reconstruct each partial-deletion crash state on disk and reopen with no resurrected keys; file count matches | needs: none | 521-556 |
| m9 | Model test with crashes and torn records | 90 | optional | random put/delete/get/scan/flush/compact/crash/tear against a BTreeMap, tally of exercised paths | several fixed seeds; assert floors on crash, torn, flush and compaction counts | needs: none; keep op count small for the 15 s debug cap | 558-732, 803-832 |
| m10 | Demo and corruption check | 45 | optional | narrated shadowing, recovery and compaction demo; corrupted table detection | assertions on the final table contents | needs: none | 734-771, 788-801, 833-840 |

**Readiness**

| Skill, concept or API | Taught by | Status | First needed |
|---|---|---|---|
| CRC32 and checksummed length-prefixed log frames | wal (STORE 2) | exists: `y9-kv-store-wal` | m1 |
| tagged length-prefixed binary records with bounds-checked decoding (checked_add, slice get, from_le_bytes) | varint-codec (BIN 1) | to be written (partly practised by `f5-varint-codec`) | m1 |
| typed error enum with From io::Error (thiserror) | error-layers (ERR 2) | staged draft: `l8-import-errors`, `l8-fix-lost-context` | m1 |
| torn-tail detection, trusting the good prefix, OpenOptions append and set_len truncation | wal (STORE 2) | exists: `y9-kv-store-wal` | m2 |
| memtable with delete markers | memtable-flush (STORE 5) | to be written | m3 |
| BTreeMap range with Bound pairs and its panic on an inverted range | memtable-flush (STORE 5) | to be written | m3 |
| Bloom filter with double hashing and a stable FNV-1a hash | bloom (CACHE 4) | exists: `f4-bloom-filter` | m4 |
| SSTable layout with sparse index, footer, Bloom block; partition_point block lookup; seek and read_exact | bloom-sstable (STORE 7) | to be written | m5 |
| temp file, sync_all, atomic rename (and parent directory fsync) | atomic-file (IO 5) | to be written | m5 |
| flush to an immutable table and layered newest-first reads where tombstones shadow | memtable-flush (STORE 5) | to be written | m6 |
| ordering durable steps (flush then log reset, publish then delete oldest first) so every crash point recovers; startup directory scan and next-id derivation | crash-ordered-files (STORE 20) *prep* | to be written | m6 |
| k-way merge with BinaryHeap and Reverse, newest wins, tombstone dropping rule | compaction (STORE 6) | staged draft: `s6-merge-sorted-streams` | m7 |
| random operations against a model | write-tests (TEST 2) | staged draft: `y5-catch-the-broken-queue` | m3 |
| injecting crashes and torn writes at chosen points | test-failpoints (TEST 7) | to be written | m9 |

<details><summary>Suspected bugs and weak spots in the Cookbook file (8 notes, all unverified: found by reading, nothing run)</summary>

- LIKELY REAL BUG, zero-filled WAL tail (replay, lines 103-117): 8 zero bytes decode as len=0 and crc=0; crc32 of the empty slice is 0, so the checksum passes, decode_entry of an empty payload returns Corrupt(short buffer), and ? makes Wal::open and Store::open fail permanently. A zero-extended tail is a common post-power-loss state and should be treated as torn. Confirmed by reading the code, not run (unverified). The model test only cuts 1-4 bytes, so it cannot catch this. Fix idea: treat len==0 as end of log.
- LIKELY REAL BUG, compact error path (lines 540-543): remove_file(...)? inside for old in self.tables.drain(..). On any delete error the early return drops the Drain, which removes ALL remaining entries, and the extend(new_table) is skipped, so the live Store has an empty table list and get/scan silently report data as missing until reopen. By inspection, not run (unverified).
- compaction crash safety (comment at 521-522) is incomplete: the merged table has no tombstones, so safety after a partial delete depends on deleting OLDEST first (the current drain order does) so a tombstone never disappears before the older value it hides; the stated reason (new table shadows old) does not cover deleted keys. The directory is also never fsynced, so unlink order is not guaranteed durable across power loss (unverified).
- durability: no parent directory fsync after rename (line 259) while wal.reset truncates the log right after, so power loss could keep the truncation and lose the table name; Wal::append never calls sync_data (comment admits), so acknowledged writes are not power-safe (unverified).
- Wal::append partial failure (line 142): if write_all fails after writing part of a frame, the log keeps the garbage, len is not rolled back, later successful appends land after it, and replay stops at the garbage, silently dropping those acknowledged writes (unverified).
- scan(from, to) with from greater than to panics inside BTreeMap::range (lines 491-493); the model test always swaps the bounds so it is never exercised (unverified by execution).
- replay stops at the first bad checksum and truncates the rest, so mid-log bit rot silently discards later valid acknowledged records; the length field is not covered by the CRC.
- the whole-file checksum is checked only in SsTable::open; later get/read_all reads are unverified. Index offsets are not range-checked, so a crafted file with a valid CRC could make end - start underflow in read_range (unverified). apply returns Err if the auto flush fails although the write is already logged and applied.

</details>

#### `cap-redis`: Mini Redis

Database servers · hard · P1 · Cookbook: exact `mini-redis`. Existing practice: b4-length-prefixed-codec. Today: engine stages yes; TCP, AOF files and threads over sockets need loopback and a writable directory (not verified).
Place: after roadmap position 400 of 435, `iv-skiplist-spans` (COLL 18), in the branch order. Series ending here: COLL, CACHE.

**Whole file read: yes.**

Lines read: `mini_redis.rs` 5876 of 5876.

**Milestones**

| # | Milestone | Min | Core? | Adds | Tested by | Runner lacks | Cookbook lines |
|---|---|---|---|---|---|---|---|
| m1 | RESP2 frames, incremental parser and encoder with limits | 60 | core | Frame and ProtoError types, Limits, strict decimal parse, parse returning need-more or consumed count, depth, bulk, array and line limits checked before waiting or allocating, encode and encode_cmd | random frame trees: every proper prefix asks for more, seeded random chunk feeds round-trip, hostile inputs give the exact error | needs: none | 63-241, 283-314, 3614-3681 |
| m2 | Server-side command parsing, arrays of bulks and inline lines | 45 | core | parse_command: inline telnet form, array whose items must all be bulk strings, null and empty arrays, ExpectedBulk errors | table of byte inputs to argument vectors, consumed counts and errors | needs: none | 242-281, 3682-3703 |
| m3 | Engine skeleton, command table and string commands without TTL | 90 | core | Value enum, typed accessors that give WRONGTYPE, table macro with name, arity, write flag and fn pointer, OnceLock index, validate and execute, string commands, negative-index range rules | call execute directly and compare reply frames, including arity, unknown command, WRONGTYPE and overflow errors | needs: none | 832-861, 1011-1019, 1029-1136, 1271-1295, 1339-1356, 1545-1610, 1692-1766 |
| m4 | Lists and hashes, with empty aggregates removed | 60 | core | non-blocking list commands and hash commands, after_write drops a key that became empty | direct execute transcripts, key disappears with its last element | needs: none | 1233-1240, 1928-2033, 2082-2165 |
| m5 | Sets and type-independent key commands | 45 | core | set commands with SINTER, SUNION, SDIFF, plus DEL, EXISTS, TYPE, RENAME, DBSIZE, FLUSHALL | direct execute transcripts incl. missing and wrong-typed keys in multi-key commands | needs: none | 1770-1790, 1838-1843, 1852-1868, 1915-1926, 2167-2222 |
| m6 | Injected clock and lazy expiry | 90 | core | Clock trait with system and manual clocks, Expires index, expire_if_needed on every access path, EXPIRE, PEXPIRE, PEXPIREAT, TTL, PTTL, PERSIST, SET options, SETEX, TTL carried by RENAME and kept by INCR | manual clock: key visible before the deadline and gone at it, TTL replies, KEEPTTL, past deadline deletes, invalid expire times | needs: none | 342-369, 863-900, 1175-1193, 1247-1267, 1601-1690, 1792-1836, 5248-5262 |
| m7 | Active expiry by random sampling | 45 | core | seeded xorshift, O(1) random choice over keys with a TTL, sample of 20 repeated while over 25 percent expired with a round bound, DEBUG SWEEP and EXPIRE-CYCLE | manual clock and fixed seed: one cycle lowers the garbage fraction below half, cycle plus sweep removes exactly the expired keys, position map stays consistent after random set and remove | needs: none | 371-385, 1195-1224, 2663-2684, 5147-5222 |
| m8 | Glob matcher and KEYS | 60 | core | token compiler for star, question mark, classes and escapes, greedy-star two-pointer matcher, KEYS filtering expired keys | fixed cases, seeded differential test against an exponential reference, pathological pattern finishes within the test cap | needs: none | 387-495, 1845-1850, 3519-3565 |
| m9 | SCAN with a last-key cursor | 45 | core | hex cursor, COUNT and MATCH, BTreeMap range with an excluded lower bound | scan with deletes and inserts mid-iteration: every stable key returned exactly once | needs: none | 327-338, 1870-1913, 5266-5312 |
| m10 | Skip list with spans: insert and delete | 120 | core | arena of nodes with a free list, random levels with p one quarter, span maintenance on insert and delete, structural checker | seeded random inserts and deletes, level-0 order equals a sorted-vector model and every span equals the real distance | needs: none | 497-650, 716-742 |
| m11 | Rank, select, score ranges and the ZSet wrapper | 60 | core | rank, get_by_rank, first_from, member-to-score map beside the list, range by index forwards and reversed, range by score with exclusive ends | seeded differential test of add, remove, rank and both range kinds against a model | needs: none | 652-717, 744-817, 3567-3612 |
| m12 | Sorted set commands | 90 | core | score parsing with infinities and NaN rejection, score formatting, exclusive bounds, ZADD through ZCOUNT with WITHSCORES and LIMIT, ZINCRBY | direct execute transcripts incl. bad floats, ties ordered by member, reverse ranks | needs: none | 819-828, 1079-1093, 1104-1112, 2224-2350 |
| m13 | Single executor thread behind a Tokio listener | 90 | core | Msg, Batch and BatchReply, executor std thread fed by a crossbeam channel, oneshot replies sent after the iteration, accept loop with JoinSet, stop signal, graceful shutdown with a drain deadline | start on port 0, one raw-socket GET of a missing key, concurrent connections, shutdown returns while a client stays connected | needs: loopback sockets (unverified); use crossbeam::channel because the runner lists crossbeam, not crossbeam_channel | 944-968, 995-1009, 1296-1298, 1358-1383, 1450-1480, 1506-1543, 3124-3218 |
| m14 | Connection loop: pipelining, protocol errors and input while waiting | 90 | core | parse every buffered command into one message, one write of all replies, keep reading under a cap while waiting, notice EOF and stop, protocol error replies then closes, QUIT | many commands in one write answered in order, a frame sent one byte at a time, good commands before bad bytes still answered, connection closed after an error | needs: loopback sockets (unverified) | 983-988, 3231-3312, 2620-2622 |
| m15 | Async client library and server commands | 60 | core | Client with timeouts, incremental reply reading, cmd and pipeline, INFO, PING, ECHO, COMMAND, info polling helpers | client round trips and pipeline against the server, INFO counters for connections and commands change as expected | needs: loopback sockets (unverified) | 932-942, 2600-2661, 3314-3436 |
| m16 | Byte-exact transcripts, protocol abuse and concurrent INCR | 60 | core | golden transcript helper over a raw socket, binary-safe large value, endless line cut off, 50 clients incrementing one counter | exact reply bytes per family, every INCR value returned exactly once (scale counts down for the debug 15 s cap) | needs: loopback sockets (unverified) | 3438-3490, 4395-4596, 4635-4813 |
| m17 | Reference model and differential command fuzzing | 120 | optional | naive second implementation of all data commands, per-type seeded command generator, run through pipelines with the manual clock advanced between batches | every reply equals the model reply and DBSIZE matches after a sweep (reduce the 120k commands for the debug cap) | needs: loopback sockets (unverified), or call the engine directly | 3706-4393, 5851-5856 |
| m18 | MULTI, EXEC and DISCARD | 60 | core | per-connection queue, QUEUED replies, validation at queue time with EXECABORT, errors inside EXEC fail alone, no nesting | transcripts for success, runtime error, queue-time error, DISCARD, EXEC without MULTI | needs: loopback sockets (unverified) | 1300-1337, 2510-2519, 2551-2596, 4591-4596 |
| m19 | WATCH optimistic transactions | 60 | core | watcher index, dirty client set fed by every write incl. delete, expiry and FLUSHALL, UNWATCH, nil EXEC on conflict | two-client conflict, each breaker command dirties the watch, concurrent compare-and-set transfers lose no update | needs: loopback sockets (unverified) | 1226-1231, 2521-2550, 4916-5030 |
| m20 | Blocking pops: parked batches served FIFO | 120 | core | BLPOP and BRPOP, parked batch keeps its remaining commands, per-key FIFO of waiters, ready list drained by settle, resume in order, no blocking inside MULTI, disconnect removes the waiter | waiters registered one at a time (poll blocked_clients) are served in arrival order, pipelined commands after BLPOP reply after it, a dropped waiter does not consume an element | needs: loopback sockets (unverified) | 970-981, 1385-1455, 2035-2080, 4815-4856, 4868-4899 |
| m21 | Executor tick: blocking timeouts and timed active expiry | 45 | core | recv_timeout based 10 ms tick, expired waiters answered earliest first, sampling cycle with a time budget, shutdown with a parked client | manual clock advance then poll for the nil reply and expired_keys, no elapsed-time bounds (the file asserts elapsed time, replace that) | needs: loopback sockets (unverified) | 1482-1504, 1524-1527, 4857-4867, 4900-4909, 5223-5246 |
| m22 | Pub/sub with channels, patterns and subscriber mode | 90 | core | bounded push queue per connection, SUBSCRIBE family, PUBLISH fan-out, multi-frame replies, command gate in subscriber mode, PING form there, select over socket and push queue, unsubscribe on disconnect | byte-exact transcript on two sockets, several subscribers get every message once and in order | needs: loopback sockets (unverified) | 1303-1314, 2352-2497, 2606-2611, 3299-3307, 4597-4634, 5032-5092 |
| m23 | Disconnect slow subscribers | 60 | core | try_send failure kills the client, Notify wakes the task even while it is blocked in a write | small queue and a reader that never reads: it is closed, the healthy subscriber gets all messages, later PUBLISH counts one (reduce payload volume for the debug cap) | needs: loopback sockets (unverified) | 2498-2508, 3220-3229, 5093-5145 |
| m24 | Effect log: what each write must record | 60 | core | records as received by default, handlers substitute deterministic records (absolute expiry, DEL for expiry and past deadline, plain pop for a served BLPOP), multi-record EXEC wrapped in MULTI and EXEC | direct execute with the manual clock: assert the exact record list for each command | needs: none | 1035-1038, 1186-1193, 1242-1267, 1346-1355, 1436-1437, 1652-1660, 1682-1688, 2059-2060, 2578-2595 |
| m25 | AOF append, group commit and replay at startup | 90 | core | Config and fsync policies always and no, flush at the end of each loop iteration before replies are released, replay through dispatch with expiry suspended, whole-database dump helper with reply rendering for comparisons | scripted units then restart gives an identical database dump, file grows after each acknowledged write, fsync counter per policy | needs: loopback sockets (unverified), writable /tmp | 904-930, 1021-1027, 1528-1531, 2907-2916, 2931-3018, 3466-3515, 5316-5418 |
| m26 | Crash recovery: torn tail and unfinished transaction | 60 | core | stop at the first incomplete record, drop an unfinished MULTI, truncate the file so appends start clean, count truncated bytes | cut the log at record boundaries, mid-record and inside MULTI: state equals the committed prefix, then append and restart again | needs: loopback sockets (unverified), writable /tmp | 2950-2990, 5456-5488 |
| m27 | everysec fsync on a helper thread | 45 | core | unsynced flag, at most one sync per second on a cloned file handle, final sync at shutdown | inject the interval or time source and poll the fsync counter, all three policies keep 30 writes across a clean restart | needs: writable /tmp, loopback sockets (unverified) | 3020-3035, 1536-1541, 5490-5533 |
| m28 | Binary snapshot codec with CRC32 | 90 | core | snapshot of live keys, magic, typed entries with optional TTL, f64 as bits, end marker and checksum, bounds-checked decoder | encode and decode round trip with infinities and binary data, seeded bit flips and truncations all rejected | needs: none | 316-325, 2689-2713, 2764-2897, 5582-5594 |
| m29 | SAVE, BGSAVE and snapshot load | 60 | core | atomic temp-write, fsync and rename, background thread reports back by message, load at startup when no AOF, seed the AOF from it, refuse a corrupt file | save, restart and compare dumps, expired key skipped, corrupt file gives InvalidData on start | needs: loopback sockets (unverified), writable /tmp | 990-993, 2899-2905, 2917-2930, 3037-3063, 3091-3096, 5535-5581, 5595-5606 |
| m30 | Background AOF rewrite | 90 | core | snapshot as a minimal chunked command stream, rewrite on a thread while new records also go to a tail buffer, append tail, sync, rename, swap handle | log of overwrites shrinks, rewrite pipelined with 200 increments loses none, restart gives the same dump | needs: loopback sockets (unverified), writable /tmp | 2715-2762, 3065-3122, 5419-5455 |
| m31 | Compare executor and sharded-lock models | 60 | optional | channel executor with and without batching against 16 mutex shards under scoped threads | assert only that every model processes all operations with correct final state, print rates | needs: parking_lot | 5608-5697 |
| m32 | Demo session and loopback throughput | 45 | optional | redis-cli style renderer, scripted sample session, pipelined load generator, per-step timeout wrapper and main | renderer golden strings, load generator checks reply counts, no rate assertions | needs: loopback sockets (unverified) | 3466-3490, 5699-5876 |

**Readiness**

| Skill, concept or API | Taught by | Status | First needed |
|---|---|---|---|
| incremental parsing of nested RESP frames (need-more versus error, consumed count) and encoding | resp (BIN 4) | to be written | m1 |
| buffer until a whole frame has arrived, then drain the consumed bytes | length-codec (BIN 2) | exists: `b4-length-prefixed-codec` | m1 |
| limits on untrusted lengths, nesting depth and line size, checked before waiting or allocating | fuzz-harness (TEST 5) | to be written | m1 |
| declarative macro_rules for a static command table and per-type accessor methods | macro-hashmap (TRAIT 6) | staged draft: `l10-fix-macro-hygiene` | m3 |
| OnceLock lazy static index | once (ATOM 2) | exists: `c3-once-cell` | m3 |
| injectable Clock trait with an atomic manual clock | fake-clock (TEST 1) | to be written | m6 |
| lazy expiry: every access path checks the deadline | ttl-kv (CACHE 2) | to be written | m6 |
| O(1) insert, remove and uniform random pick (Vec, swap_remove, position map) | sampled-expiry (CACHE 10) *prep* | to be written | m6 |
| seeded xorshift RNG | sampled-expiry (CACHE 10) *prep* | to be written | m7 |
| active expiry: sample, delete, repeat above a threshold with round and time bounds | sampled-expiry (CACHE 10) *prep* | to be written | m7 |
| greedy-star glob matching without exponential backtracking | glob (PARSE 10) | to be written | m8 |
| differential test against a slow reference implementation | test-differential (TEST 6) | to be written | m8 |
| resumable cursor made from the last key, BTreeMap range with Bound and borrowed keys | cursor-pagination (WEB 2) | to be written | m9 |
| hex encode and decode | base64 (TEXT 3) | to be written | m9 |
| index-based node arena with a free list | slab (MEM 1) | exists: `f3-generational-slab` | m10 |
| skip list search, random levels, insert and delete | ordered-map-skiplist (COLL 14) | to be written | m10 |
| skip list link spans: maintenance, rank and select by rank | skiplist-spans (COLL 18) *prep* | to be written | m10 |
| two indexes over one set (member to score, plus score order) kept consistent | leaderboard (COLL 11) | to be written | m11 |
| model plus seeded random operations to find bugs | write-tests (TEST 2) | staged draft: `y5-catch-the-broken-queue` | m11 |
| one thread owns all state and is reached by messages over a channel (crossbeam recv_timeout and try_recv) | actor (CONC 16) | to be written | m13 |
| bridging a std thread and Tokio: oneshot send and bounded mpsc try_send from sync code, unbounded sync channel from async code, spawn_blocking to join | async-sync-bridge (ASYNC 14) *prep* | to be written | m13 |
| JoinSet, watch stop signal, drain with a deadline then abort_all | graceful-shutdown (ASYNC 7) | staged draft: `c4-drain-with-deadline` | m13 |
| Tokio TcpListener and TcpStream, into_split, read_buf, write_all, set_nodelay, time::timeout | tokio-tcp-conn (NET 9) *prep* | to be written | m13 |
| pipelining: parse all buffered commands, one request, one write | tokio-tcp-conn (NET 9) *prep* | to be written | m14 |
| select! over socket and reply or push channel with a branch precondition and a cap on buffered input | tokio-tcp-conn (NET 9) *prep* | to be written | m14 |
| cancel safety of futures dropped by select! | cancel-safety (ASYNC 6) | staged draft: `c4-fix-cancel-safe-reader` | m14 |
| optimistic concurrency: abort at commit if a watched item changed | occ (TXN 5) | to be written | m19 |
| parked requests as stored continuations: per-key FIFO waiters on several keys, cleanup from every queue, ready list, resume without blocking the owner thread | parked-waiters (CONC 21) *prep* | to be written | m20 |
| deadlines for waiting entries checked on a tick, fired in time order with cancellation | job-scheduler (CONC 15) | to be written | m21 |
| pub/sub fan-out with bounded queues and a policy for slow readers | pubsub (CONC 12) | to be written | m22 |
| Tokio Notify as a kill signal raced against a stalled write_all | tokio-tcp-conn (NET 9) *prep* | to be written | m23 |
| logging deterministic effects in place of requests so replay needs no clock or blocking | effect-log (STORE 17) *prep* | to be written | m24 |
| transaction boundary markers in a log so replay is all or nothing | effect-log (STORE 17) *prep* | to be written | m24 |
| group commit: drain queued requests, append once, sync, then release replies | group-commit (STORE 18) *prep* | to be written | m25 |
| fsync policies, sync_data against sync_all, File try_clone and background sync | group-commit (STORE 18) *prep* | to be written | m25 |
| replay of the good prefix and truncation of a torn tail with set_len | wal (STORE 2) | exists: `y9-kv-store-wal` | m26 |
| crash testing by cutting the log at chosen points | test-failpoints (TEST 7) | to be written | m26 |
| CRC32 | wal (STORE 2) | exists: `y9-kv-store-wal` | m28 |
| tagged length-prefixed binary records with little-endian integers and f64 to_bits | varint-codec (BIN 1) | to be written (partly practised by `f5-varint-codec`) | m28 |
| bounds-checked decoding of hostile bytes and seeded corruption testing | fuzz-harness (TEST 5) | to be written | m28 |
| atomic temp file, sync_all and rename | atomic-file (IO 5) | to be written | m29 |
| online log rewrite: point-in-time snapshot on a thread, buffer the tail, splice and swap | online-log-rewrite (STORE 19) *prep* | to be written | m30 |
| std::thread::scope worker fan-out | parallel-mapreduce (CONC 13) | to be written | m31 |
| sharding one lock into many (comparison model) | sharded-map (CONC 10) | to be written | m31 |
| throughput measurement with warm-up | bench-harness (PERF 1) | to be written | m32 |

<details><summary>Suspected bugs and weak spots in the Cookbook file (16 notes, all unverified: found by reading, nothing run)</summary>

Later note: the Cookbook file has since changed, so these notes and the line numbers above describe the old version. Bugs from these notes were fixed with checks in `t_regressions`; what remains is listed under LIMITS in the file header.

- set_expire_at (1252) deletes on a past deadline even while loading. Replaying PEXPIREAT k t after t, followed by later writes made while k was alive (RPUSH, INCR) and no logged DEL (server stopped before expiry), recreates k with only the later data and no TTL. Real Redis does not delete on past deadlines while loading. Also affects the rewrite tail. Tests miss it because the manual clock is not advanced before restart (unverified, found by reading)
- BGREWRITEAOF is queueable in MULTI and calls flush_aof inside EXEC (3077), draining the log, so self.log.len() - mark (2589) underflows when earlier unflushed records existed, e.g. one pipeline SET x 1, MULTI, BGREWRITEAOF, EXEC. Debug build panics and kills the executor, hence the whole server. With mark 0 the transaction is written without its MULTI wrapper (unverified)
- block (1386): BLPOP k 1e300 passes the finite check, the cast saturates to u64::MAX and self.now + timeout_ms overflows: debug panic on the executor thread, release wraps to an immediate timeout (unverified)
- validate (1281-1284) puts the raw command name into a simple error frame and encode does not strip CR or LF, so a bulk command name containing CRLF injects extra reply lines and desynchronises the client; the subscriber-mode error (1310) has the same shape (unverified)
- parse_command inline branch checks max_line only when no newline is buffered (248-254), so an over-long line that arrives with its newline is accepted and the limit depends on packet chunking; take_line also rejects a max_line line split between CR and LF (unverified)
- per-connection input is unbounded in practice: MAX_PENDING applies only while awaiting a reply (3275), and the outer read (3297) grows inbuf up to max_array times max_bulk (about 64 GiB) for one incomplete command; the incomplete frame is also reparsed and recopied from the start on every read, quadratic for large arrays (unverified)
- c_save does not check bg_save and write_atomic always uses path.with_extension("tmp") (2900), so SAVE during BGSAVE has two writers on one temp file: possible corrupt or stale snapshot; AOF and RDB paths sharing a stem can also collide (unverified)
- flush_aof (3005-3017) ignores write_all and sync_data errors after draining the records, so replies report success for writes that were lost even under Always; a partial write leaves a torn record mid-file and replay then truncates everything appended after it (unverified)
- bg_done: if reopening fails after the rename (3109-3110) the engine keeps appending to the old unlinked file and those writes are lost; neither this nor write_atomic fsyncs the parent directory; final sync_all of the rewritten file runs on the executor thread (unverified)
- load_persistence seeds the AOF from the RDB with a non-atomic fs::write (2928); a crash mid-seed leaves a non-empty partial AOF that wins over the snapshot at next start. load_aof also treats any mid-file corruption as a torn tail and silently truncates all later data (unverified)
- EXEC does not run lazy expiry on watched keys before testing dirtiness (2563-2577), so a watched key that expired untouched and unsampled does not abort the transaction (unverified)
- blocked clients are served only after the pushing batch finishes (settle at 1479), so a later LPOP in the same pipeline takes the element from the waiter; ready is popped as a stack, reversing key order; RENAME onto a waited key never wakes waiters. All differ from Redis (unverified)
- kill_client increments slow_kills whenever the client entry exists (2506), so a subscriber matched by both a channel and a pattern in one PUBLISH with a full queue is counted twice (unverified)
- messages already in the push queue are written after the UNSUBSCRIBE acknowledgement, so a client back in normal mode can receive stray pushes between command replies (unverified)
- tests assert wall-clock behaviour (BLPOP timeout window at 4864, real-timer expiry, everysec) and deadlines use the non-monotonic system clock; the model run (120k commands), 64 MB slow-subscriber traffic and 25 s step timeouts are unlikely to fit a 15 s debug test cap (unverified)
- runner mismatch: the file uses let chains (edition 2024, e.g. 1497, 2910, 3303) and the crossbeam_channel crate name, while the runner is edition 2021 and lists crossbeam. The executor also holds its own Sender, so the Disconnected arm (1522) is unreachable and a Server dropped without shutdown leaks the thread; accept errors retry in a busy loop (3183) (unverified)

</details>

#### `cap-optimizer`: Query optimizer

Query engines · hard · P2 · Cookbook: exact `query-optimizer`. Existing practice: none. Today: yes.
Place: after roadmap position 435 of 435, `iv-alg-co-window-deque-dp` (COMBO 10), in the branch order. Series ending here: QUERY, DP, COMBO.

**Whole file read: yes.**

Lines read: `query_optimizer.rs` 2236 of 2236.

**Milestones**

| # | Milestone | Min | Core? | Adds | Tested by | Runner lacks | Cookbook lines |
|---|---|---|---|---|---|---|---|
| m1 | Expression tree and evaluator | 60 | core | Expr enum and builders, flatten and rebuild conjuncts, column references, table bitmask, row eval with a NULL sentinel, normalising col-op-literal | table tests over hand-built rows, including NULL comparisons and flipped literals | needs: none | 27-31,177-301 |
| m2 | Seeded data and in-memory database | 60 | core | xorshift RNG, Zipf sampler, table specs, rows, sorted (key,rowid) indexes that skip NULL, small random database generator | same seed gives identical data; index is sorted and complete; Zipf frequencies ordered on a fixed seed | needs: none | 33-80,419-478,526-550 |
| m3 | ANALYZE column statistics | 60 | core | NDV, min, max, null fraction, equi-height histogram, most common values, CDF with interpolation inside a bucket, catalog metadata | hand-computed stats on tiny columns; frac_le monotone and within a bound of the true fraction on seeded data; all-NULL column | needs: none | 82-175 |
| m4 | Cardinality estimation | 90 | core | equality, range, IN, AND with same-column range merge, OR, equi-join selectivity, default selectivity, clamping, q-error | estimate against true counts: bounded q-error on uniform data, large on a correlated pair, MCV beats 1/ndv on a hot value | needs: none | 303-417,1852-1858 |
| m5 | Logical plan, physical plan and naive lowering | 60 | core | Logical enum, schema and table mask per node, naive plan from a query, Plan node with rows, cost and schema, cost constructors for SeqScan, Filter, Project, NestedLoop | golden schemas; exact rows and cost for hand-built plans | needs: none | 552-584,777-857,874-900,914-924,1059-1076 |
| m6 | Volcano executor for naive plans | 90 | core | RowIter trait with boxed borrowing children; SeqScan, Filter, Project, NestedLoop with the inner side materialised once; run to a Vec | hand-written expected multisets on tiny tables | needs: none | 1510-1526,1539-1601,1661-1671,1711-1717,1730-1747,1773-1781 |
| m7 | Random query generator and differential harness | 60 | core | connected random equi-join queries with extra filters and constant subexpressions, sorted-multiset comparison against the naive plan | determinism by seed; every query is connected; some results are non-empty | needs: none | 1783-1838 |
| m8 | Rewrite rules to a fixpoint | 90 | core | constant folding, conjunction splitting, predicate pushdown and join extraction, bottom-up pass, fixpoint loop with fired counters | golden logical trees; results equal the naive plan on seeded queries; estimated cost never rises after these three rules; rewrite is idempotent | needs: none | 586-672,726-775 |
| m9 | Projection pruning | 60 | optional | second rule phase that inserts Projects to drop unused columns early (the final optimize pipeline of the file does not use it) | golden trees; results unchanged on seeded queries; second run changes nothing | needs: none | 674-724,731,2176-2192 |
| m10 | Access path selection and ordered scans | 90 | core | IndexScan and Sort costing, sargable range extraction with residual filter, leaf plan alternatives, IndexScan and blocking Sort execution | selective predicate picks the index and unselective picks SeqScan; every leaf alternative returns the same multiset | needs: none | 859-872,902-912,1010-1057,1527-1538,1642-1660,1718-1729,1764-1771 |
| m11 | Hash join and merge join execution | 90 | core | build and probe iterator with residual predicate, merge join over duplicate runs, NULL keys never match | both against nested loop on seeded tables with duplicates and NULLs, with and without residual | needs: none | 1602-1641,1672-1709,1748-1763 |
| m12 | Join candidates and interesting orders | 90 | core | NestedLoop, HashJoin and MergeJoin candidates with costs, Sort added only when needed, plan set keeping the cheapest plan per output order | zero Sorts over index-ordered inputs and two over heap scans; a pricier ordered plan survives pruning; cheapest hash join builds the smaller side | needs: none | 926-1008 |
| m13 | Join graph and per-subset cardinality | 60 | core | flatten a rewritten tree into leaves with local filters and multi-table edges, cardinality for every subset, joining two plan sets with cross products refused | golden graph for a fixed query; cardinality is the same for every split of a subset; unconnected sets yield no plan unless allowed | needs: none | 1078-1221 |
| m14 | Dynamic programming join ordering and optimize | 90 | core | submask splits for left-deep and bushy, DP over subsets, cross-product fallback, full optimize pipeline | optimised results equal naive on seeded queries over tiny tables; bushy cost never above left-deep | needs: none; keep case counts small for the 15 s debug cap (unverified) | 1223-1274,1393-1402,1840-1850,2164-2202 |
| m15 | EXPLAIN output | 45 | core | expression and operator printing, indented plan tree with rows and cost, logical tree printing | golden strings for fixed plans | needs: none | 1404-1508 |
| m16 | Brute-force join tree oracle | 60 | optional | enumerate every ordered binary tree and cost each with the same operator choices | DP cost equals the minimum for 3 to 5 tables, with and without cross products | needs: none | 1330-1391,2132-2162 |
| m17 | Greedy operator ordering | 45 | optional | repeatedly join the connected pair with the smallest result, in both orientations | same rows as DP; cost never below DP on seeded 8 to 10 table queries | needs: none | 1276-1328,2208-2229 |
| m18 | Shop schema demo report | 90 | optional | skewed and correlated demo schema, q-error report, index crossover table, join cost table, 4-table EXPLAIN and execution | assertions on q-error bounds, scan choices and equal results across DP shapes and greedy | needs: none; 20000-row data may be slow in debug (unverified) | 479-524,1860-2131,2230-2236 |

**Readiness**

| Skill, concept or API | Taught by | Status | First needed |
|---|---|---|---|
| seeded xorshift RNG, Zipf by cumulative weights and binary search, Fisher-Yates, skewed and correlated generated data | seeded-data-gen (TEST 11) *prep* | to be written | m2 |
| recursion over a boxed expression enum (flatten, collect references, evaluate) | alg-ast-walk (REC 2) | to be written | m1 |
| binding a column reference to a row position and evaluating predicates with NULL | expr-eval-3vl (QUERY 1) | to be written | m1 |
| set of table ids as a bitmask in one integer | alg-bit-flags (BIT 1) | to be written (partly practised by `f2-tcp-flags-in-a-byte`) | m1 |
| equi-depth histogram, NDV, null fraction and range estimate by interpolation | cardinality (QUERY 12) | to be written | m3 |
| most common values list for skewed columns | cardinality (QUERY 12) | to be written | m3 |
| independence assumption, correlated columns and q-error | cardinality (QUERY 12) | to be written | m4 |
| selectivity of a predicate tree: OR, IN, same-column range intersection, equi-join 1 over max NDV | predicate-selectivity (QUERY 14) *prep* | to be written | m4 |
| pull-based operator trait with boxed children that borrow the database | volcano (QUERY 2) | to be written | m6 |
| random query generation checked against a naive plan, compared as sorted multisets | test-differential (TEST 6) | to be written | m7 |
| constant folding that preserves meaning | constant-fold (LANG 2) | to be written | m8 |
| logical plan tree rewriting, predicate pushdown and rules run to a fixpoint | planner-rules (QUERY 9) | to be written | m8 |
| projection pruning | planner-rules (QUERY 9) | to be written | m9 |
| choosing index scan or sequential scan by estimated cost | planner-rules (QUERY 9) | to be written | m10 |
| sargable conjuncts merged into one index range plus a residual filter | predicate-selectivity (QUERY 14) *prep* | to be written | m10 |
| blocking operator that runs on first next (boxed FnOnce, then drain a Vec iterator) | volcano (QUERY 2) | to be written | m10 |
| hash join build and probe with duplicates and residual predicate | hash-join (QUERY 5) | to be written | m11 |
| sort-merge join over duplicate key runs | merge-join (QUERY 6) | to be written | m11 |
| cost formulas for competing physical operators, sort enforcer, cheapest plan kept per output order | interesting-orders (QUERY 15) *prep* | to be written | m12 |
| join graph, subset cardinality and connectivity to avoid cross products | join-order (QUERY 13) | to be written | m13 |
| bitmask DP over bushy and left-deep splits with submask enumeration | join-order (QUERY 13) | to be written | m14 |
| exhaustive enumeration as an oracle for the DP | test-differential (TEST 6) | to be written | m16 |
| greedy join ordering compared with DP | join-order (QUERY 13) | to be written | m17 |

<details><summary>Suspected bugs and weak spots in the Cookbook file (9 notes, all unverified: found by reading, nothing run)</summary>

- eval handles Add and Mul before the NULL arm, so NULL + 2 wraps into an ordinary integer and later comparisons can be true; random queries only add literals, so tests miss it (lines 261-264, unverified by running)
- optimize runs only PHASE1 and collect() looks through Project, so projection pruning never reaches the physical plan (lines 1098-1101,1394-1401)
- the claim that rewrites never raise estimated cost is asserted only for phase 1; phase 2 adds Project nodes that cost CPU in this model, so it would likely raise cost (lines 2176-2182, unverified)
- the brute-force oracle reuses join_sets and PlanSet, so it checks only split enumeration, not pruning or cost formulas
- bounds computes v+1 or v-1 without checks: col > i64::MAX overflows in debug, and col &lt; NULL+1 produces the NULL sentinel (lines 403-410, unverified)
- ColStats::analyze on an empty table divides 0 by 0, giving a NaN null fraction (line 138, unverified)
- MergeJoin output is marked ordered only by the left key although it is also ordered by the equal right key, so a later merge may add a needless Sort
- index scan caps random page fetches at the table page count, optimistic for an unclustered index that revisits pages (line 863)
- tables are identified by table id, so self-joins cannot be expressed; masks are u32 and cardinalities are stored for all 2^n subsets

</details>

#### `cap-ratelimitlab`: Rate limiter lab

Resilience · medium · P2 · Cookbook: exact `rate-limiter-lab`. Existing practice: none. Today: yes (time is an argument).
Place: after roadmap position 350 of 435, `iv-aimd-concurrency` (RESIL 12), in the branch order. Series ending here: RESIL.

**Whole file read: yes.**

Lines read: `rate_limiter_lab.rs` 681 of 681.

**Milestones**

| # | Milestone | Min | Core? | Adds | Tested by | Runner lacks | Cookbook lines |
|---|---|---|---|---|---|---|---|
| m1 | Limiter trait, fixed window, sliding log and the window meter | 60 | core | time-as-argument trait with state_bytes, two exact-count algorithms, two-pointer max-in-any-window and decision replay | boundary trace admits 2x limit for fixed window and exactly limit for the log; meter equals a quadratic count on seeded traces | needs: none | 26-112, 447-482 |
| m2 | Sliding window counter | 45 | core | weighted previous plus current estimate with window roll | hand-computed decisions at window fractions; long idle gap resets; boundary trace stays below 2x | needs: none | 114-156 |
| m3 | Token bucket and leaky meter in integer time | 45 | core | credit and level formulations | burst of capacity then exact refill rate; both give identical decisions on seeded traces | needs: none | 158-231, 604-612 |
| m4 | GCRA and equivalence | 60 | core | single theoretical-arrival-time state, tolerance | decisions identical to token bucket and leaky meter over many seeded inline traces and several burst/interval pairs | needs: none | 233-262, 614-631 |
| m5 | Per-key limiter with idle eviction | 60 | core | factory closure, HashMap entry, periodic retain sweep, safe idle bound | noisy tenant held to its quota, quiet tenants never rejected, 1000 one-shot keys swept to 1 | needs: none | 275-312, 633-677 |
| m6 | Lock-free concurrency limiter with RAII permit | 60 | core | CAS-loop admission on an atomic, Arc shared state, permit Drop | single-thread cap checks; threaded stress asserts peak never above max and zero leaked permits; panic releases permit | needs: threads (standard) | 314-351, 370-375, 514-550 |
| m7 | AIMD adaptive limit | 45 | core | additive increase and halving, clamped | deterministic round simulation: limit settles near capacity and follows a capacity drop | needs: none | 352-369, 552-582 |
| m8 | Traffic generators and comparison lab | 60 | optional | dyn lineup, steady, bursty, boundary and tenant traffic, printed table, cross-algorithm assertions | fixed-seed assertions on max-in-window per algorithm | needs: none | 264-273, 377-445, 483-512, 584-603 |

**Readiness**

| Skill, concept or API | Taught by | Status | First needed |
|---|---|---|---|
| inject time as an argument for deterministic tests | fake-clock (TEST 1) | to be written | m1 |
| fixed window, sliding log and weighted sliding window counter | window-limiters (RESIL 10) *prep* | to be written | m1 |
| maximum events in any half-open window (two-pointer sweep) | alg-iv-sweep (IVL 2) | to be written | m1 |
| token bucket with lazy integer refill | rate-limiter (RESIL 2) | staged draft `m2-rate-limiter` (new in build-your-own) | m3 |
| leaky bucket meter and GCRA, and their equivalence to the token bucket | gcra-equivalence (RESIL 11) *prep* | to be written | m3 |
| per-key state with idle expiry (HashMap entry, retain) against an injected clock | ttl-kv (CACHE 2) | to be written | m5 |
| guard returns a permit on Drop | semaphore (CONC 5) | exists: `c1-fair-semaphore` | m6 |
| AtomicUsize fetch_update CAS loop with orderings for bounded admission | aimd-concurrency (RESIL 12) *prep* | to be written | m6 |
| AIMD adaptive concurrency limit | aimd-concurrency (RESIL 12) *prep* | to be written | m7 |
| atomic high-water mark with fetch_max across threads | atomic-counters (MON 1) | exists: `c3-atomic-stats` | m6 |
| seeded xorshift traffic generation | write-tests (TEST 2) | staged draft: `y5-catch-the-broken-queue` | m8 |

<details><summary>Suspected bugs and weak spots in the Cookbook file (4 notes, all unverified: found by reading, nothing run)</summary>

- try_acquire (lines 336-343) loads limit once outside the CAS loop, so if on_overload lowers it concurrently an acquire can still admit against the old higher limit (unverified; no test changes the limit while threads acquire).
- TokenBucket and LeakyMeter set last_ms = now_ms even if time goes backwards, which later credits the same interval twice; SlidingCounter::roll, KeyedLimiter sweep and retain subtract unchecked and panic in debug on backwards time. The trait documents non-decreasing time but nothing enforces it.
- Gcra::new with burst 0 underflows (line 243). KeyedLimiter does not enforce idle_ms >= recovery time, so a short idle_ms hands a swept key a fresh full quota (documented only in a comment).
- Gcra::state_bytes reports only tat (8 bytes) while the other limiters report full struct size including their config, so the bytes/key column is not like for like.

</details>

#### `cap-raytracer`: Ray tracer

Graphics · medium · P3 · Cookbook: exact `ray-tracer`. Existing practice: none. Today: math and image stages yes; rendering is slow in a debug test run (15 s cap), so tests must use tiny images.
Place: after roadmap position 421 of 435, `iv-simd-scan` (PERF 4), in the branch order. Series ending here: PERF, NUM.

**Whole file read: yes.**

Lines read: `ray_tracer.rs` 1716 of 1716.

**Milestones**

| # | Milestone | Min | Core? | Adds | Tested by | Runner lacks | Cookbook lines |
|---|---|---|---|---|---|---|---|
| m1 | Vector math and per-pixel RNG | 45 | core | Vec3 with operators, dot, cross, norm, reflect, componentwise ops; splitmix64 with float in [0,1) and a stream per (seed,x,y) | unit tests on algebraic identities (cross orthogonal to inputs, norm has length 1); same key gives same stream, different pixels differ, floats stay in range | needs: none | 41-161 |
| m2 | Ray against sphere, triangle and plane | 60 | core | Ray, sphere quadratic with t interval, Moller-Trumbore, plane hit, normals, primitive bounds | hand-computed hits and misses, origin inside sphere returns far root, seeded random rays: reported point lies on the surface | needs: none | 206-215, 230-243, 277-330, 549-558 |
| m3 | Scene with brute-force closest hit | 60 | core | materials enum, scene lists, closest hit with equal-t tie broken by lower index, planes tested separately, any-hit occlusion, Hit record, environment radiance | overlapping duplicate primitives give lowest index; reordering far objects does not change the hit; occluded respects tmax | needs: none | 217-275, 454-502, 539-583 |
| m4 | Pinhole camera and single-thread renderer | 60 | core | camera basis and field of view, pixel to ray, stratified jittered or centred samples, framebuffer of linear colour showing albedo or environment | centre ray points at the target; unjittered 33x25 render has the expected object at known pixels; same seed gives identical buffer | needs: none | 695-731, 753-770, 965-993 |
| m5 | Linear colour to a PNG file | 60 | core | sRGB encoding with NaN and range clamp, CRC-32, Adler-32, stored-deflate zlib stream, PNG chunks | CRC-32 and Adler-32 check values; a test decoder walks chunks and stored blocks and recovers the pixels, including data over 65535 bytes | needs: none (write under /tmp) | 1018-1099 |
| m6 | Whitted diffuse shading with hard shadows | 90 | core | Lambert, checker, Blinn-Phong, point lights with inverse-square falloff, shadow rays with origin offset, ambient term | floor point behind a sphere is below 0.2 of a lit point and above zero; centre pixel of a red sphere is dominantly red; checker cells alternate for negative coordinates | needs: none | 772-792, 849-899 |
| m7 | Mirror and glass recursion | 90 | core | recursion with depth limit, mirror tint, Snell refraction, total internal reflection, Schlick Fresnel mix, Beer-Lambert absorption, emissive | mirror pixel equals 0.9 times the emitter colour within 1e-9; refract at normal incidence is straight; angle past critical returns None | needs: none | 188-201, 793-848 |
| m8 | Area lights and soft shadows | 45 | core | parallelogram lights sampled several times per shading point | point on penumbra has visibility strictly between umbra and lit values with a fixed seed; zero-size area light equals the point light | needs: none | 250-256, 858-895 |
| m9 | Thin-lens depth of field | 45 | core | rejection sampling on a disk, lens origin offset, aim at the focus-plane point | 50 lens rays for one (s,t) meet at one point on the focus plane within 1e-9; aperture 0 equals pinhole | needs: none | 180-187, 732-747 |
| m10 | Quads, rotated boxes and icosphere meshes | 60 | core | quad as two triangles, box rotated about y, icosahedron subdivision with a shared-midpoint cache and outward winding | triangle count is 20*4^levels, every vertex is on the sphere, every edge is shared by two faces, every normal points outward | needs: none | 584-690 |
| m11 | Build a flat BVH | 90 | core | node bounds, centroid bounds, median split on longest axis with index tie-break, depth-first flat Vec | each primitive appears in exactly one leaf; child boxes lie inside parents; leaf size at most 4; depth is logarithmic; two builds are equal | needs: none | 332-433, 483-485 |
| m12 | Traverse the BVH | 90 | core | slab test with a shrinking limit, explicit stack, near child first, equal-t tie preserved, any-hit early exit | seeded random scenes and rays: BVH result equals brute force exactly; small rendered image is pixel-identical both ways | needs: none | 434-452, 503-548 |
| m13 | Multithreaded rendering | 45 | core | scoped threads pulling disjoint row bands from a locked iterator | images with 1, 4 and 7 threads are bit-identical | needs: none | 995-1016 |
| m14 | Path tracer | 90 | core | iterative integrator with throughput, orthonormal basis, cosine-weighted hemisphere directions, mirror, stochastic glass, emissive and environment | sampled directions are unit and on the correct side, mean cosine near 2/3 with a fixed seed; albedo-1 and mirror spheres in a uniform environment give exactly 1 | needs: none | 163-179, 901-953, 962-963 |
| m15 | Russian roulette and white furnace | 60 | core | termination after 3 bounces with survival probability and compensation | furnace at tiny resolution: albedo 0.5 gives 0.5 within tolerance, lossless glass gives 1 within 0.01, all values finite and non-negative | needs: none (keep images tiny for the 15 s debug cap) | 954-961, 1562-1604 |
| m16 | Cornell box and convergence | 60 | core | Cornell scene lit only by an emissive panel; RMSE helper | fixed seeds, small image: RMSE against a higher-sample reference falls at each 4x step | needs: none (cookbook uses 40x40 at 1024 spp; shrink for the debug 15 s cap) | 1274-1353, 1355-1386, 1606-1644 |
| m17 | Showcase scenes and gallery | 45 | optional | Whitted and glass showcase scenes, final renders, ASCII preview | preview has the requested rows and columns; all pixels finite | needs: none (full-size renders need a release build) | 1087-1123, 1128-1272, 1672-1714 |
| m18 | BVH speed comparison | 45 | optional | time BVH against brute force on a few thousand triangles and report the ratio | images equal; the ratio is printed, not asserted | needs: release build for meaningful numbers | 1646-1670 |

**Readiness**

| Skill, concept or API | Taught by | Status | First needed |
|---|---|---|---|
| operator overloading on a numeric value type (Add, Sub, Mul by scalar, Neg, AddAssign) | generic-matrix (TRAIT 3) | to be written (partly practised by `l4-matrix-operators`, `l5-shape-typed-matrix`) | m1 |
| seeded splitmix64, float from top 53 bits, independent stream per work item | seeded-rng (TEST 8) *prep* | to be written | m1 |
| dot, cross, normalise, reflection formula | ray-intersection (NUM 3) *prep* | to be written | m1 |
| ray-sphere quadratic, plane equation, Moller-Trumbore within a (tmin,tmax) interval | ray-intersection (NUM 3) *prep* | to be written | m2 |
| floating-point robustness: epsilon offsets, degenerate determinants, f64::next_up, deterministic tie-breaking, NaN behaviour of min, max and partial_cmp | float-robust (NUM 4) *prep* | to be written | m2 |
| camera basis and field of view to primary rays | camera-rays (NUM 5) *prep* | to be written | m4 |
| stratified jittered supersampling | monte-carlo (NUM 8) *prep* | to be written | m4 |
| CRC-32 and Adler-32 | checksums (BIN 12) *prep* | to be written | m5 |
| PNG chunks, zlib header, stored deflate blocks, big and little endian fields | png-container (BIN 13) *prep* | to be written | m5 |
| linear to sRGB transfer function and quantising to u8 | png-container (BIN 13) *prep* | to be written | m5 |
| Lambert and Blinn-Phong terms, inverse-square falloff, shadow rays | direct-shading (NUM 6) *prep* | to be written | m6 |
| Snell refraction, total internal reflection, Schlick Fresnel, Beer-Lambert | surface-optics (NUM 7) *prep* | to be written | m7 |
| area light sampling as a Monte Carlo average | monte-carlo (NUM 8) *prep* | to be written | m8 |
| thin-lens model with rejection sampling on a disk | camera-rays (NUM 5) *prep* | to be written | m9 |
| mesh generation: icosphere subdivision with canonical edge keys, winding, rotation about an axis | mesh-subdivision (NUM 9) *prep* | to be written | m10 |
| bounding volume hierarchy: AABB, median split, flat depth-first layout | bvh-flat (COLL 17) *prep* | to be written | m11 |
| recursive split of one slice into two mutable halves (split_at_mut) | split-borrow-bank (OWN 1) | to be written (partly practised by `l2-get-disjoint-mut`) | m11 |
| AABB slab test and stack traversal with pruning | bvh-flat (COLL 17) *prep* | to be written | m12 |
| differential testing of accelerated against brute-force results | test-differential (TEST 6) | to be written | m12 |
| std::thread::scope with results independent of thread count | parallel-mapreduce (CONC 13) | to be written | m13 |
| work queue of disjoint mutable chunks (Mutex around chunks_mut().enumerate()) | chunk-queue (CONC 20) *prep* | to be written | m13 |
| orthonormal basis and cosine-weighted hemisphere sampling, pdf cancellation | monte-carlo (NUM 8) *prep* | to be written | m14 |
| Russian roulette as an unbiased estimator | monte-carlo (NUM 8) *prep* | to be written | m15 |
| white furnace energy test, RMSE and 1/sqrt(N) convergence checks | monte-carlo (NUM 8) *prep* | to be written | m15 |
| micro-benchmark measurement | bench-harness (PERF 1) | to be written | m18 |

<details><summary>Suspected bugs and weak spots in the Cookbook file (7 notes, all unverified: found by reading, nothing run)</summary>

- line 1667 asserts a wall-clock speed-up above 3x; a timing assertion that can fail on a loaded machine or debug build (unverified)
- lines 705-726: Camera::new yields NaN when the view direction is parallel to world up (zero cross product); not rejected (unverified)
- lines 1050-1063: encode_png with h = 0 emits no deflate block (invalid stream), and w = 0 panics in chunks(0); only reachable by misuse (unverified)
- lines 904 and 954-961: the path tracer is described as unbiased but the max_depth cut-off drops energy (small bias) (unverified)
- lines 902-963: the path tracer ignores sc.lights (no next-event estimation) and renders Blinn as pure Lambert; a Whitted scene looks different or darker under Mode::Path (unverified)
- lines 874-884: shadow rays treat glass and emissive geometry as opaque; area lights have no emitter-cosine or area term (documented approximation only in part) (unverified)
- lines 1637-1643: the header claims RMSE falls like 1/sqrt(N) but only monotonic decrease and a 2.5x cut over 1 to 64 spp are checked

</details>

#### `cap-regex`: Regex engine

Compilers and search · hard · P1 · Cookbook: exact `regex-nfa-engine`. Existing practice: y8-regex-nfa-matcher (one-step version). Today: yes.
Place: after roadmap position 303 of 435, `iv-sbx-fuel` (SBX 5), in the branch order. Series ending here: none.

**Whole file read: yes.**

Lines read: `regex_nfa_engine.rs` 652 of 652.

**Milestones**

| # | Milestone | Min | Core? | Adds | Tested by | Runner lacks | Cookbook lines |
|---|---|---|---|---|---|---|---|
| m1 | AST and recursive-descent parser for the operators | 60 | core | literals, dot, anchors, star/plus/optional, alternation, groups, positioned errors | pattern to expected tree table; each error variant with its position | needs: crate thiserror (vendored) | 44-171 |
| m2 | Character classes and escapes | 60 | core | negation, ranges, literal dash, class and top-level escapes, range errors | membership table including bracket edge cases and every documented escape (include backslash s and t, inside and outside classes) | needs: none | 25-42, 172-210 |
| m3 | Thompson construction | 90 | core | state vector with index ids, back-to-front emission with a patched Split for loops | structural check of small patterns; state count at most pattern length plus 1, also for long repeated patterns | needs: none | 212-282, 355-367, 644-650 |
| m4 | Set simulation and whole-string match | 90 | core | epsilon closure with visited bitmap, conditional anchor edges, step, is_match | demo table; nested empty loops terminate; pathological pattern on 100000 chars returns false (size lowered if needed for the debug time cap) | needs: none | 284-329, 368-370, 547-560 |
| m5 | Leftmost-longest find | 60 | core | try each start, track last accepting position, char indices | find table including anchors, empty match and non-ASCII text | needs: none | 331-353, 371-374, 561-583 |
| m6 | Continuation-passing backtracking oracle with step budget | 90 | optional | AST matcher with continuations, budget error, empty-iteration guard, exhaustive find using Cell | agrees with a hand table; exhausts the budget on the pathological patterns with a long run of a | needs: none; bound input length since the budget does not bound stack depth | 376-492 |
| m7 | Random differential test | 60 | optional | grammar-based random pattern and text generators, state-count bound | thousands of seeded pairs agree for is_match and find, with floors on compared and matched counts | needs: none; reduce counts for the 15 s debug cap | 494-545, 585-625 |
| m8 | Catastrophic backtracking comparison | 45 | optional | side-by-side demonstration of budget exhaustion against linear NFA | oracle returns StepLimit while NFA returns false | needs: none | 627-643 |

**Readiness**

| Skill, concept or API | Taught by | Status | First needed |
|---|---|---|---|
| recursive descent with precedence levels over a char vector | recursive-descent (PARSE 11) | to be written | m1 |
| typed parse errors carrying positions (thiserror) | error-layers (ERR 2) | staged draft: `l8-import-errors`, `l8-fix-lost-context` | m1 |
| bracket class, range and escape syntax with its edge cases | regex-syntax (PARSE 20) *prep* | to be written | m2 |
| Thompson NFA construction, epsilon closure and simultaneous-state simulation | regex-nfa (PARSE 15) | exists: `y8-regex-nfa-matcher` | m3 |
| anchors as position-dependent epsilon edges, and leftmost-longest unanchored find | nfa-anchors-search (PARSE 21) *prep* | to be written | m4 |
| continuation-passing backtracking with dyn Fn continuations | cps-backtracker (PARSE 22) *prep* | to be written | m6 |
| step budget to stop runaway execution | sbx-fuel (SBX 5) | to be written (partly practised by `y6-wasm-stack-interpreter`) | m6 |
| Cell for mutation inside a Fn closure | my-refcell (SMART 2) | to be written | m6 |
| differential testing with seeded random regex source generation | test-differential (TEST 6) | to be written | m7 |

<details><summary>Suspected bugs and weak spots in the Cookbook file (4 notes, all unverified: found by reading, nothing run)</summary>

- LIKELY REAL BUG: the header (line 6) promises backslash s and backslash t, but escape() (202-210) handles only d, w and n; backslash s matches a literal s and backslash t a literal t. By inspection, not run (unverified). The random generator emits no escapes, so tests miss it.
- inside a class (line 182) an escape becomes the raw next char, so [\d] matches only d and [\n] only n; a range end (line 189) takes no escape, so a backslash is used as the bound itself (unverified).
- parser, compiler and AST drop recurse with no depth limit, so deeply nested groups or a long run of quantifiers can overflow the stack although the header promotes untrusted patterns (unverified). The oracle step budget also does not bound recursion depth.
- stale comments: closure mentions a DFA cache that does not exist and blocked anchor states kept for later re-closing that no caller uses. find is O(n^2 * states), documented at line 332, weaker than the header's O(text * states) claim; closure allocates a bitmap per step.

</details>

#### `cap-search`: Search engine

Search · hard · P2 · Cookbook: exact `search-engine`. Existing practice: y10-trigram-index, y10-bm25-ranking, f7-posting-list-codec. Today: yes.
Place: after roadmap position 342 of 435, `iv-immutable-segments` (SEARCH 17), in the branch order. Series ending here: SEARCH.

**Whole file read: yes.**

Lines read: `search_engine.rs` 940 of 940.

**Milestones**

| # | Milestone | Min | Core? | Adds | Tested by | Runner lacks | Cookbook lines |
|---|---|---|---|---|---|---|---|
| m1 | Tokenizer, varint and positional posting codec | 60 | core | Unicode-aware lowercase tokenizer; LEB128 put/get; delta-coded posting lists with errors for truncated, over-long and trailing input | seeded random lists round-trip; every proper prefix errors; hand byte vectors | needs: none | 23-107, 743-766 |
| m2 | Immutable segment and term lookup | 60 | core | Segment build (doc lengths, lowercase texts, term to compressed postings), Index add_segment over several segments, postings, doc_len; seeded corpus generator | term to doc ids and positions equals a brute-force token scan over a seeded corpus, for one and many segments | needs: none | 144-206, 422-429, 463-509, 630-689 |
| m3 | Phrase search | 45 | core | match consecutive positions across per-term posting maps | 300 seeded phrases (half lifted from documents) equal a windows() scan; non-empty hit count floor | needs: none | 515-541, 787-812 |
| m4 | Query lexer and parser | 90 | core | quotes, parentheses, NOT over AND over OR, implicit AND, wildcard words, Expr tree, typed parse errors | table of query to expected tree; each error variant; unbalanced quote cases including a lone trailing quote | needs: crate thiserror (vendored) | 252-405 |
| m5 | Trigram filter then verify | 90 | core | byte-trigram index per segment, wildcard piece matcher, rarest-first sorted intersection, fallback for short pieces | seeded substrings equal str::contains over all docs; candidate count never below true matches; short and non-ASCII needles | needs: none | 109-142, 177-185, 225-249, 543-552, 814-838 |
| m6 | Boolean evaluation with a brute-force oracle | 60 | core | evaluate Expr to a sorted doc set (And, Or, Not against all docs, Phrase, Wild) | 400 seeded random queries equal an oracle that shares no index code | needs: none | 511-513, 554-564, 612-628, 691-711, 768-785 |
| m7 | BM25 ranking and top-k | 90 | core | global-statistics BM25 over positive terms, Ord on a float score with doc-id tie break, bounded min-heap | hand-built four-doc corpus ordering; b=0 gives ties; top-k equals full sort for seeded scores | needs: none | 407-461, 566-595, 715-741 |
| m8 | Segment merge | 90 | core | merge adjacent segments by re-encoding postings and appending trigram lists; merge_all | merged segment equals a freshly built one; rankings identical before and after merging and across segmentations | needs: none | 207-223, 486-493, 840-879 |
| m9 | Size report, filter precision and demo | 45 | optional | compressed against plain byte counts, trigram precision, sample queries | compressed is at least 2x smaller on the seeded corpus | needs: none | 104-107, 597-608, 881-939 |

**Readiness**

| Skill, concept or API | Taught by | Status | First needed |
|---|---|---|---|
| LEB128 varint encode and checked decode | varint-codec (BIN 1) | to be written (partly practised by `f5-varint-codec`) | m1 |
| delta (gap) encoding of sorted ids and positions | posting-compression (SEARCH 2) | exists: `f7-posting-list-codec` | m1 |
| seeded xorshift RNG and random-input tests against a model | write-tests (TEST 2) | staged draft: `y5-catch-the-broken-queue` | m1 |
| inverted index: term to documents, built from tokens | inverted-index (SEARCH 1) | to be written (partly practised by `y10-trigram-index`) | m2 |
| per-document positions and phrase matching by position offset (binary_search) | positional-phrase (SEARCH 16) *prep* | to be written | m3 |
| immutable segments over contiguous doc-id ranges, global ids and global statistics | immutable-segments (SEARCH 17) *prep* | to be written | m2 |
| lexer with quoted mode using a peekable char iterator (by_ref take_while) | shell-line (PARSE 8) | exists: `y9-shell-line-parser` | m4 |
| recursive descent with one function per precedence level, implicit AND | recursive-descent (PARSE 11) | to be written | m4 |
| typed errors with thiserror derive | error-layers (ERR 2) | staged draft: `l8-import-errors`, `l8-fix-lost-context` | m4 |
| trigram index with filter-then-verify and rarest-first intersection | trigram (SEARCH 4) | exists: `y10-trigram-index` | m5 |
| wildcard matching of ordered literal pieces | glob (PARSE 10) | to be written | m5 |
| boolean query evaluation as set operations (BTreeSet, NOT against the universe) | boolean-query (SEARCH 3) | to be written | m6 |
| differential test against a brute-force oracle with random recursive queries | test-differential (TEST 6) | to be written | m6 |
| BM25 formula (idf, tf saturation, length normalisation) | bm25 (SEARCH 5) | exists: `y10-bm25-ranking` | m7 |
| top-k with BinaryHeap and Reverse; total_cmp based Ord with tie break; into_sorted_vec | bm25 (SEARCH 5) | exists: `y10-bm25-ranking` | m7 |
| merging segments: concatenating postings and re-encoding deltas, proving invisibility to ranking | immutable-segments (SEARCH 17) *prep* | to be written | m8 |

<details><summary>Suspected bugs and weak spots in the Cookbook file (5 notes, all unverified: found by reading, nothing run)</summary>

- lex (lines 292-298): the unterminated-quote test only fires if the query does not END with a quote, so a lone trailing opening quote (e.g. rust followed by one double quote) is accepted as an empty phrase (matches nothing under implicit AND) instead of UnterminatedQuote. Found by inspection, not run (unverified).
- get_varint (46-53): on the fifth byte, payload bits above bit 31 are silently shifted out, so overflowing encodings are accepted with a wrong value. decode_postings uses unchecked += on doc and position gaps, which panics in a debug build on hostile bytes. unverified by execution.
- oracle_match calls the same wildcard_match used by the index verify step, so the boolean differential is not independent for wildcard queries (check_substrings with str::contains covers only star-free needles; multi-piece patterns have no independent oracle).
- performance weak spots: doc_len does a linear segment search per posting; bm25_scores re-sums all doc lengths and re-decodes postings every query; posting decode and set evaluation materialise everything; merge_all clones and re-encodes cumulatively (quadratic in segment count).
- positive_terms skips everything under any NOT, so NOT NOT rust matches like rust but every score is 0 (unverified whether intended).

</details>

#### `cap-sql`: Tiny SQL engine

Databases · hard · P1 · Cookbook: exact `tiny-sql-engine`. Existing practice: none. Today: yes.
Place: after roadmap position 391 of 435, `iv-seeded-data-gen` (TEST 11), in the branch order. Series ending here: ITER, PARSE.

**Whole file read: yes.**

Lines read: `tiny_sql_engine.rs` 1577 of 1577.

**Milestones**

| # | Milestone | Min | Core? | Adds | Tested by | Runner lacks | Cookbook lines |
|---|---|---|---|---|---|---|---|
| m1 | Values, errors and caret rendering | 45 | core | Value and type enums with derived ordering, Display, thiserror error enum with spans, compiler-style rendering | golden renderings, including multi-line and end-of-input spans | needs: none | 27-115 |
| m2 | SQL lexer with spans | 60 | core | case-folded identifiers, integers with range error, quoted strings with doubled quotes, symbols, positioned errors | token and span tables; error spans for unterminated string and bad character | needs: none | 117-193 |
| m3 | Pratt expression parser | 90 | core | AST, token cursor helpers, binding powers for OR, AND, NOT, comparison, IS NULL, arithmetic and unary minus | parenthesised tree printing goldens for precedence and associativity | needs: none | 195-219,270-329,452-528 |
| m4 | Statement parser | 90 | core | CREATE TABLE, INSERT with many tuples, SELECT with items, aggregates, WHERE, GROUP BY, ORDER BY, LIMIT, trailing-input check | golden statements; exact error spans and messages | needs: none | 220-268,330-451,530-550 |
| m5 | Binder, type checker and three-valued evaluation | 90 | core | names to indices, static types with NULL compatible with all, NULL logic, checked arithmetic and division by zero | truth tables; type errors raised with no rows present; overflow and divide errors are values | needs: none | 552-706 |
| m6 | Database with scan, filter and project | 90 | core | catalog, all-or-nothing INSERT, operator trait, Scan, Filter, Project, plain planner and execute | end-to-end queries with NULLs; failed INSERT appends nothing; catalog errors | needs: none | 708-735,779-794,881-906,930-958,1016-1083 |
| m7 | Sort and Limit | 45 | core | blocking stable sort ascending and descending, Limit that stops pulling, placed before Project | stable ties; NULL placement; LIMIT 0 over 1/0 returns no error | needs: none | 737-777,908-928 |
| m8 | Aggregation and GROUP BY | 90 | core | accumulators for COUNT, SUM, MIN, MAX, NULL rules, one row for empty input, output layout, aggregate planner and its checks, ORDER BY on output names | tables including empty input, NULL group, SUM overflow and invalid select lists | needs: none | 796-879,960-1014 |
| m9 | Acceptance corpus | 45 | core | the hand-checked query set and the full error-kind corpus run against the whole engine, with fixes | each query has an exact expected result; each bad statement its error variant and span | needs: none | 1139-1314 |
| m10 | Differential testing on random tables | 90 | optional | random predicate trees that print as SQL and evaluate in plain Rust, random tables, sort, limit and group oracles | 800 seeded tables or fewer agree with the oracle; failures print the SQL | needs: none; iteration count may need lowering for the 15 s debug cap (unverified) | 1119-1137,1316-1544 |
| m11 | Table rendering and script demo | 45 | optional | aligned result tables and a scripted session that prints errors with carets | golden table output | needs: none | 1085-1117,1546-1577 |

**Readiness**

| Skill, concept or API | Taught by | Status | First needed |
|---|---|---|---|
| thiserror enum with structured variants as the engine error | error-layers (ERR 2) | staged draft: `l8-import-errors`, `l8-fix-lost-context` | m1 |
| byte span to line, column and caret underline | diagnostics-renderer (ERR 1) | to be written | m1 |
| tokens with byte spans, UTF-8 safe string literals with escapes, errors on unknown input | tokenizer-borrowed (PARSE 2) | exists: `l3-zero-copy-tokenizer` | m2 |
| Pratt parsing with binding powers for prefix, infix and postfix | pratt (PARSE 12) | staged draft: `y14-pratt-parser` | m3 |
| recursive descent over SQL statements, keywords against identifiers, errors with spans | sql-parser (QUERY 7) | to be written | m4 |
| bind column names to indices and type check before reading rows | expr-eval-3vl (QUERY 1) | to be written | m5 |
| three-valued logic and checked arithmetic returned as errors | expr-eval-3vl (QUERY 1) | to be written | m5 |
| planning a parsed SELECT into operators, ORDER BY on unselected columns, ORDER BY on output names after aggregation | mini-sql-exec (QUERY 8) | to be written | m6 |
| pull operators behind a trait returning Result of Option, with LIMIT stopping early | volcano (QUERY 2) | to be written | m6 |
| blocking stable sort operator | volcano (QUERY 2) | to be written | m7 |
| grouped accumulators for count, sum, min and max | hash-agg (QUERY 4) | to be written | m8 |
| random predicate trees checked against an independent oracle | test-differential (TEST 6) | to be written | m10 |
| seeded xorshift table and predicate generation | seeded-data-gen (TEST 11) *prep* | to be written | m10 |

<details><summary>Suspected bugs and weak spots in the Cookbook file (6 notes, all unverified: found by reading, nothing run)</summary>

- parser, binder and evaluator recurse without a depth limit: many nested parentheses, repeated NOT or a very long operator chain would overflow the stack, against the claim that user input cannot panic the engine (lines 453-509,581-627,691-706, unverified)
- CREATE TABLE accepts duplicate column names; the second can never be referenced (lines 341-355,1031-1036)
- ORDER BY accepts only an identifier and there are no aliases, so a result cannot be ordered by an aggregate such as count(*) (lines 423-428,1004-1011)
- plan clones every table row before execution, so LIMIT saves operator work but not the copy (lines 891-893)
- AND and OR evaluate both sides, so false AND 1/0 = 1 raises a runtime error (line 704; a semantics choice worth stating)
- the minimum i64 value cannot be written as a literal because the positive part is lexed first

</details>

#### `cap-fs`: Toy filesystem

Operating systems · hard · P3 · Cookbook: exact `toy-filesystem`. Existing practice: none. Today: yes with a simulated block device; real-file stage needs a writable tmp.
Place: after roadmap position 422 of 435, `iv-alg-bit-xor` (BIT 4), in the branch order. Series ending here: IO, OS, BIT.

**Whole file read: yes.**

Lines read: `toy_filesystem.rs` 2745 of 2745.

**Milestones**

| # | Milestone | Min | Core? | Adds | Tested by | Runner lacks | Cookbook lines |
|---|---|---|---|---|---|---|---|
| m1 | Byte codecs and an in-memory block device | 45 | core | little-endian field helpers, error enum, 4096-byte block type, block device trait with bounds-checked memory disk | round trips at offsets; out-of-range block numbers return errors, never panic | needs: none | 47-103, 157-168, 220-257 |
| m2 | File-backed block device | 45 | core | create and open an image file, positional read and write, set length, sync | write blocks, drop, reopen, read back; past-end access is an error | needs: unix FileExt and a writable /tmp | 170-218, 2653-2659 |
| m3 | CRC32 and the superblock | 90 | core | const-built chainable CRC32, geometry to layout computation, superblock encode and decode with magic, version and checksum checks | known CRC vector and chaining law; bad magic, flipped byte, tiny device and bad geometry give the typed errors | needs: none | 105-134, 482-597 |
| m4 | Write-back buffer cache | 60 | core | LRU by stamp, dirty tracking, dirty blocks never evicted, flush and discard | a counting device shows when reads and writes reach it; dirty blocks survive over capacity; discard restores old contents | needs: none | 370-480 |
| m5 | Bitmaps, next-fit allocator and format of metadata | 90 | core | zero metadata, pre-set metadata bits, bit get and put, next-fit scan with wrap and full-byte skip, block alloc and free with double-free check, free count | allocate to exhaustion, free and reallocate; random alloc and free compared with a set | needs: none | 745-771, 953-1041 |
| m6 | Inodes, transactions and mount | 90 | core | 128-byte inode codec, inode alloc and free, link count adjust, injected clock, transaction that commits by flushing or aborts by discarding dirty blocks, plain mount | inode round trip; a failing closure leaves the device image unchanged; inode exhaustion is typed; damaged inode rejected | needs: none | 599-643, 649-700, 851-902, 1043-1098 |
| m7 | File block mapping | 90 | core | logical to disk block through 12 direct, single and double indirect pointers, allocate on demand, pointer range checks | map blocks at every region boundary and count allocations; holes map to 0; a pointer outside the data area is Corrupt | needs: none | 1100-1155 |
| m8 | File data with holes | 60 | core | block-granular read and read-modify-write, sparse reads as zeros, size and mtime updates, maximum size check | unaligned writes across block edges compared with a byte vector; reading a hole allocates nothing | needs: none | 1244-1290 |
| m9 | Directories and the root | 60 | core | 64-byte entries in a directory file, add with slot reuse, remove, replace, name validation, dot and dotdot, root created by format | add and remove sequences list the right names and reuse slots; malformed entry bytes give Corrupt | needs: none | 702-740, 772-779, 1292-1355 |
| m10 | Paths, create, mkdir, read, write, stat, readdir | 90 | core | component split, walk, parent resolution, public operations as transactions, writes split into chunk-sized transactions, block listing for stat, whole-tree dump | feature script with exact typed errors; nlink after mkdir; dump equals a literal tree; remount gives the same dump | needs: none | 1156-1184, 1357-1451, 1580-1653 |
| m11 | Truncate and unlink with block reclamation | 90 | core | free every block at or past a cut inside indirect trees, zero the kept tail, drop an inode | free-block count returns to baseline after files reaching double indirect; shrink then grow reads zeros; ENOSPC leaves old files intact and space reusable | needs: none | 1185-1239, 1387-1391, 1452-1494, 2441-2545 |
| m12 | rmdir, hard links and rename | 90 | core | link counts, last-link deletion, directory move with dotdot fix, refusal to move into own subtree, replace-target rules | table of every rename case with exact errors and link counts | needs: none | 1495-1579, 2319-2439 |
| m13 | Redo journal and recovery | 120 | core | header and block images, sync, checksummed commit record, sync, checkpoint, mark clean with sequence number; replay or discard at mount; transaction too big | build journal states by hand (no commit, bad checksum, valid commit, bad targets) and mount; replaying twice gives the same image | needs: none | 645-647, 785-875, 904-951 |
| m14 | Fault-injecting disk | 60 | core | volatile write list that sync makes durable, crash after k writes, post-crash image that keeps all or shuffles, drops and tears 512-byte sectors by seed | direct tests of the device: synced data always survives; same seed gives the same image | needs: none | 136-155, 259-368 |
| m15 | fsck checks | 90 | core | walk from the root, count references, find cross-links, orphans, bad dot entries, bitmap differences | plant each kind of damage through transactions and expect the matching problem; healthy tree is clean | needs: none | 1655-1800, 1840-1851, 2547-2577, 2581-2597 |
| m16 | fsck repair | 60 | core | fix list applied in small transactions for link counts, orphans, missing and leaked bits | after repair a second run is clean; cross-links are reported and left alone | needs: none | 1801-1839, 2578-2580 |
| m17 | Crash at every write boundary | 90 | core | for a scripted list of operations, crash at each write in both modes and also during recovery; require old or new state and a clean fsck | recovered tree equals the tree before or after; both outcomes and at least one replay occur | needs: a small script and image; the full 40-sequence run is unlikely to fit the 15 s debug cap (unverified) | 2218-2316 |
| m18 | Hostile images never panic | 60 | core | mount, fsck, dump and mutate junk and randomly corrupted images and get only typed errors | fixed seeds: no panic and no hang; specific corruptions give the expected error | needs: guard huge allocations and cycles, or a bad size field can exhaust memory | 2599-2651 |
| m19 | Model-based random operations | 90 | optional | path-map reference model, random operation generator, compare results, trees, fsck and remounts | seeded run: same success or failure per step and equal dumps | needs: far fewer than 6000 steps for the 15 s debug cap (unverified) | 1853-2216 |
| m20 | Real-file demo and run summary | 45 | optional | persistence scenario on a real image with self-removing temp file, cache and journal statistics | reopen and compare bytes, link counts and sparse tail | needs: writable /tmp | 2660-2745 |

**Readiness**

| Skill, concept or API | Taught by | Status | First needed |
|---|---|---|---|
| fixed-offset little-endian codec for headers, with magic, version and validation | layout-quiz-codec (UNSAFE 1) | to be written (partly practised by `y3-repr-c-header`, `f2-fix-packed-zip-header`) | m1 |
| positional file I/O as a block device (read_exact_at, write_all_at, set_len, OpenOptions, sync_data) | positional-block-io (IO 8) *prep* | to be written | m2 |
| table-driven CRC32 built in a const fn and extended over chunks | checksums (BIN 12) *prep* | to be written | m3 |
| write-back page cache where pages in use cannot be evicted (no-steal dirty blocks) | buffer-pool (STORE 9) | to be written | m4 |
| recency-based eviction | lru (CACHE 1) | staged draft: `f4-lru-cache-arena` | m4 |
| block bitmaps and block allocation for a file system | fs-blocks (OS 5) | to be written | m5 |
| injected clock for deterministic timestamps | fake-clock (TEST 1) | to be written | m6 |
| all-or-nothing operation by buffering writes and discarding on error; redo journal with commit record, two barriers, checkpoint, sequence numbers and idempotent replay | redo-journal (STORE 16) *prep* | to be written | m6 |
| torn and partial writes detected by checksum, trust only what verifies | wal (STORE 2) | exists: `y9-kv-store-wal` | m13 |
| inode block map: direct and indirect pointers, holes, read-modify-write, freeing the tail of a pointer tree | inode-block-map (OS 7) *prep* | to be written | m7 |
| path resolution over a tree addressed by names | file-system-tree (PAT 11) | to be written | m10 |
| UNIX namespace semantics: directory entries separate from inodes, link counts, unlink, rename rules and subtree cycle check | unix-namespace (OS 8) *prep* | to be written | m9 |
| disk write-cache fault model: unsynced writes lost, reordered or torn; sync as a barrier | disk-fault-model (TEST 10) *prep* | to be written | m14 |
| seeded xorshift RNG and Fisher-Yates shuffle | seeded-rng (TEST 8) *prep* | to be written | m14 |
| reachability from a root compared with the allocation map, to find leaks and orphans | mark-sweep (MEM 5) | to be written | m15 |
| crash injection at every chosen point with a before-or-after oracle | test-failpoints (TEST 7) | to be written | m17 |
| hostile on-disk input: validate every number, never panic | fuzz-harness (TEST 5) | to be written | m18 |
| reference model plus random operations | write-tests (TEST 2) | staged draft: `y5-catch-the-broken-queue` | m19 |

<details><summary>Suspected bugs and weak spots in the Cookbook file (10 notes, all unverified: found by reading, nothing run)</summary>

- unverified: if commit fails with a device error, dirty cache entries are neither discarded nor is the Fs poisoned (tx only discards when the closure fails). The failed operation stays visible and is committed by the next one; after the commit record, jseq is also not advanced (884-951)
- unverified: fsck repair is destructive when the walk is incomplete. A directory whose entries do not parse, or an inode with a bad block tree, leaves children or blocks out of reached and owner, so repair frees those inodes and clears bits of blocks still in use (1697-1703, 1665-1675, 1769-1821)
- unverified: a directory that fails to load is not marked reached, so repair frees its inode while the parent entry still points at it (1688-1695, 1769-1774)
- unverified: dump has no visited set, and rename's walk up dotdot has no step limit; a corrupted image with a directory cycle makes either loop forever (1621-1650, 1532-1542)
- unverified: a corrupted size field up to MAX_FILE_SIZE is accepted, and dir_raw or dump then allocates about 4 GiB (1056, 1249, 1299)
- unverified: Super::layout accepts journal_blocks 4 and 5, but formatting the root directory appears to need about four block images, so format would fail with TxTooBig (525, 909-912, 773-777)
- weak spot: the header says each public operation is atomic, but write splits into several transactions, so a large write that hits NoSpace or a crash is partly applied; the ENOSPC test only uses a single-chunk write (1431-1442, 2518-2521)
- weak spot: write calls stat, which walks the whole block tree, so each write costs time proportional to file size (1421, 1583)
- weak spot: a data write does not change ctime, unlike POSIX, and the feature test asserts that behaviour (1288, 2432)
- weak spot: fsck does not repair wrong dot or dotdot entries and does not detect duplicate names in a directory

</details>

#### `cap-lox`: Interpreter with a bytecode VM

Languages · hard · P1 · Cookbook: exact `toy-language-interpreter`. Existing practice: y6 (WebAssembly interpreter). Today: yes, but large for one lib.rs.
Place: after roadmap position 407 of 435, `iv-alg-memo-resolver` (REC 6), in the branch order. Series ending here: OWN, LANG, REC.

**Whole file read: yes.**

Lines read: `toy_language_interpreter.rs` 3572 of 3572.

**Milestones**

| # | Milestone | Min | Core? | Adds | Tested by | Runner lacks | Cookbook lines |
|---|---|---|---|---|---|---|---|
| m1 | Spans and diagnostics | 45 | core | Span, diagnostic type, byte offset to line and column, one-line form and caret rendering | exact strings for hand-made spans, including multi-byte text before the span and a span at end of input | needs: none | 37-112 |
| m2 | Scanner | 90 | core | tokens with spans, keywords, one and two character operators, numbers, strings with escapes, line and nested block comments, errors collected while scanning goes on | token and span tables; unterminated string and comment, bad escape, non-ASCII stray character give exact diagnostics and no panic | needs: none | 114-377 |
| m3 | Expression parser | 90 | core | expression AST, binding-power loop for infix operators, unary, call and property chains, grouping, right associative assignment with invalid-target error | s-expression dump of parsed trees checks precedence and associativity; bad targets and missing parens give exact diagnostics | needs: none | 379-438, 479-563, 760-861 |
| m4 | Statement parser | 90 | core | var, block, if, while, return, break, continue, function declarations, 255 limit, for lowered to a block plus while with a separate increment slot | AST dumps; for with each clause missing; exact messages per missing token | needs: none | 440-474, 582-648, 668-758 |
| m5 | Class syntax and panic-mode recovery | 60 | core | class and superclass syntax, synchronise at statement boundaries so one run reports many errors | a source with three independent syntax errors yields exactly three diagnostics in order | needs: none | 565-606, 649-667, 3066-3074 |
| m6 | Resolver: lexical scopes | 90 | core | scope stack, declare then define, hop count stored in the AST through Cell, global fallback, duplicate local and read-in-own-initialiser errors | dump of hop counts per name for shadowing and closure sources; exact error lines | needs: none | 863-967, 1003-1007, 1011-1022, 1060-1077 |
| m7 | Resolver: context rules and warnings | 60 | core | return, break, this, super and initializer rules with saved and restored context, synthetic this and super scopes, unused-local warnings, front end that sorts diagnostics and stops on errors | table of source to exact diagnostics; warnings do not block the result | needs: none | 968-1002, 1023-1059, 1078-1108, 2578-2605, 3011-3065, 3382-3408 |
| m8 | Runtime values and operators | 60 | core | value enum, truthiness, equality by value or identity, number formatting, depth-bounded list printing, shared checked arithmetic and runtime error type | operator table over value pairs with exact results and messages; self-containing list prints and returns | needs: none | 1110-1262, 1427-1439 |
| m9 | Tree-walker: expressions and variables | 90 | core | parent-linked environments, globals, lookup and assign by hop count, blocks, short circuit, runtime errors with line | programs observed through a temporary print hook or final globals; shadowing and undefined-variable messages exact | needs: none | 1441-1490, 1501-1600, 1674-1731 |
| m10 | Control flow and natives | 60 | core | if, while with increment, break and continue as a flow value, native function table with arity ranges, list natives and print | exact output for loop programs including continue in for; every native error and arity message | needs: none | 1264-1425, 1491-1496, 1601-1621, 1786-1792 |
| m11 | Functions, closures, traces and limits | 90 | core | closures over environments, call arity, call frames and stack trace innermost first, call depth limit, step fuel | exact output for counter and mutual recursion; exact traces through three frames; runaway recursion and loop end in typed errors | needs: tests must run the interpreter on a spawned thread with a large stack, because a debug build uses far more than the 2 MB test thread at depth 400 | 1497-1556, 1622-1629, 1733-1741, 1815-1847, 2917-2923, 3411-3440 |
| m12 | Classes, methods and super | 120 | core | class values, instances with fields, method lookup up the chain, binding this, init returning the instance, super, property errors | exact output for hierarchy and bound-method programs; superclass-not-a-class, missing property and constructor arity errors | needs: same large-stack thread as m11 | 1139-1156, 1630-1672, 1742-1785, 1793-1814, 1849-1858 |
| m13 | Driver and golden program suite | 45 | core | source to outcome (output, runtime error, compile errors, warnings) and a table of whole programs | fib, sieve, sorts, loop-closure capture and every error case match exact output and trace | needs: same large-stack thread | 2561-2644, 2646-3009, 3343-3355, 3373-3381 |
| m14 | Compiler and VM: expressions and globals | 90 | core | instruction enum, constants, per-instruction lines, stack machine loop, global define get and set, operators shared with the tree-walker | expression and global programs give the same output and errors as the expected strings; classes reported unsupported | needs: none | 1860-1896, 1953-1986, 2060-2108, 2160, 2219-2248, 2272-2297, 2304-2332, 2388-2440, 2455-2472, 2554-2559 |
| m15 | Locals, scopes and jumps | 120 | core | compile-time local slots, scope exit pops, if and short circuit with back-patched jumps, while and for, break and continue that pop inner locals first | golden control-flow programs; break and continue out of nested blocks with locals leave later reads correct | needs: none | 1909-1930, 1972-1979, 1988-2036, 2109-2139, 2164-2191, 2249-2264, 2473-2479 |
| m16 | VM calls, traces and limits | 90 | core | function prototypes, call frames with base slot, return, native calls, stack traces from saved instruction pointers, depth limit and fuel | non-capturing function programs; traces identical to the tree-walker ones; overflow trace has every frame; fuel stops a loop | needs: none | 1931-1951, 2140-2159, 2192-2218, 2265-2271, 2299-2303, 2333-2360, 2500-2550 |
| m17 | Upvalues and closures in the VM | 120 | core | resolve captures through enclosing functions, shared open upvalues pointing at stack slots, close on scope exit and return | closure counter, shared capture between two closures, loop-variable capture rule and closures that outlive their frame match exact output | needs: none | 1897-1908, 2037-2058, 2361-2387, 2441-2454, 2480-2499, 3356-3368 |
| m18 | Random program generator and differential test | 120 | optional | seeded generator of valid terminating programs, run on both back ends and compare output and errors | fixed seeds: no compile errors, identical output and error text; counts of both successful and failing programs are non-zero | needs: keep the program count small enough for the 15 s debug cap (300 is unverified) | 3078-3338, 3442-3470 |
| m19 | Demo and speed comparison | 45 | optional | sample scripts, caret listing and a fib timing printout for both back ends | sample outputs asserted exactly; timing printed only | needs: release build for meaningful timing | 3472-3572 |

**Readiness**

| Skill, concept or API | Taught by | Status | First needed |
|---|---|---|---|
| byte span to line and column, caret rendering | diagnostics-renderer (ERR 1) | to be written | m1 |
| scanning into tokens with byte spans | tokenizer-borrowed (PARSE 2) | exists: `l3-zero-copy-tokenizer` | m2 |
| quoted strings with escapes as a separate lexer mode | shell-line (PARSE 8) | exists: `y9-shell-line-parser` | m2 |
| UTF-8 character boundaries (len_utf8, safe slicing of source by byte offset) | utf8-codec (TEXT 1) | to be written | m1 |
| recursive descent over a token stream | recursive-descent (PARSE 11) | to be written | m3 |
| Pratt parsing with binding powers and associativity | pratt (PARSE 12) | staged draft: `y14-pratt-parser` | m3 |
| statement grammar, assignment-target conversion and lowering sugar (for into while) | stmt-parser-desugar (PARSE 19) *prep* | to be written | m3 |
| panic-mode error recovery and synchronisation points | error-recovery (PARSE 18) | to be written | m5 |
| Cell and RefCell interior mutability (AST annotations, shared lists, fields, environments) | my-refcell (SMART 2) | to be written | m6 |
| static scope resolution into hop counts | tree-interpreter (LANG 4) | to be written | m6 |
| static semantic checks with saved and restored context, rejection before running | typecheck-simple (LANG 3) | to be written | m7 |
| parent-linked Rc environments and closures that capture them | sexpr-lisp (LANG 1) | to be written | m9 |
| tree-walking evaluation with break, continue and return as control-flow results | tree-interpreter (LANG 4) | to be written | m10 |
| interpreter call frames, stack traces, depth limit and a sized host thread (thread::Builder::stack_size) | interp-call-frames (LANG 13) *prep* | to be written | m11 |
| step fuel that meters execution | sbx-fuel (SBX 5) | to be written (partly practised by `y6-wasm-stack-interpreter`) | m11 |
| object model: method resolution up a superclass chain, bound methods, this, super, init | interp-object-model (LANG 14) *prep* | to be written | m12 |
| instruction set, constant pool and stack VM dispatch loop | bytecode-vm (LANG 5) | to be written (partly practised by `f2-eight-byte-instructions`) | m14 |
| forward jump back-patching and loop jumps | bytecode-vm (LANG 5) | to be written (partly practised by `f2-eight-byte-instructions`) | m15 |
| compile-time local slots that mirror the runtime stack, scope-exit pops on break and continue | vm-local-slots (LANG 15) *prep* | to be written | m15 |
| VM call frames with base slot, saved instruction pointer and line tables | interp-call-frames (LANG 13) *prep* | to be written | m16 |
| upvalues: open and closed captured variables | vm-upvalues (LANG 16) *prep* | to be written | m17 |
| differential testing with two engines as oracles | test-differential (TEST 6) | to be written | m18 |
| seeded xorshift RNG | seeded-rng (TEST 8) *prep* | to be written | m18 |
| grammar-based generation of programs that are valid and terminate by construction | grammar-program-gen (TEST 9) *prep* | to be written | m18 |

<details><summary>Suspected bugs and weak spots in the Cookbook file (7 notes, all unverified: found by reading, nothing run)</summary>

- unverified: the resolver does not check assignment to a local inside its own initialiser. For { var a = (a = 1); } the tree-walker ignores the failed Env::set silently, while the VM has no local yet, emits SetGlobal and raises Undefined variable, so the two back ends diverge (1074-1077, 1696-1699, 2100-2107)
- unverified: scanner invalid escape followed by a multi-byte character makes a span ending inside that character (end = i + 1) and skips only one byte; Diag::render then slices src at a non-boundary and panics (280-303, 101)
- unverified: neither parser, resolver, compiler nor AST drop has a nesting limit, so deeply nested or very long operator-chain input overflows the Rust stack and aborts; MAX_CALL_DEPTH only covers script calls
- unverified: stringify limits depth to 8 but not total size, so a list that holds itself many times prints exponentially; fuel does not bound this or string growth (1190-1204)
- unverified: the VM emits line 0 for many instructions (constants, pops, jumps, loops), so a fuel-exhaustion trace can say line 0 and differ from the tree-walker (2098-2127, 2184, 2221-2233, 2406-2409)
- weak spot: Env::get and Env::set failures fall back to nil or are ignored, which hides resolver and interpreter mismatches (1677, 1698)
- weak spot: tree-walk depth 400 relies on the 256 MB thread made in main; the same calls on a default test thread would likely abort (unverified)

</details>

#### `cap-typeahead`: Typeahead service

Search and web · medium · P2 · Cookbook: exact `typeahead-service`. Existing practice: none. Today: engine stages yes; HTTP stage needs loopback or oneshot.
Place: after roadmap position 391 of 435, `iv-seeded-data-gen` (TEST 11), in the branch order. Series ending here: TEXT.

**Whole file read: yes.**

Lines read: `typeahead_service.rs` 1871 of 1871.

**Milestones**

| # | Milestone | Min | Core? | Adds | Tested by | Runner lacks | Cookbook lines |
|---|---|---|---|---|---|---|---|
| m1 | Text normalisation | 45 | core | lowercase, accent table, combining mark removal, whitespace collapse | table of case, accent, decomposed, CJK and spacing inputs | needs: none | 155-204 |
| m2 | Bounded edit distance with transpositions | 60 | core | optimal string alignment with three rolling rows and early cutoff, whole-term or prefix distance, typo budget by length | tables; bounded version agrees with the full version on seeded strings | needs: none | 206-266 |
| m3 | Matching predicate over padded trigrams | 60 | core | packed trigram keys, padded and unpadded gram sets, minimum shared-gram threshold, prefix, infix and typo classification | table of query and term pairs per match kind, including short queries | needs: none | 268-332 |
| m4 | Term table, decayed popularity and total order | 60 | core | injectable clock, add and event operations merged by normalised key, forward-decay weight, rank comparison with full tie-breaks | manual clock: recent selections overtake old ones; order is total on seeded terms; unknown event changes nothing | needs: none | 69-100,334-415 |
| m5 | Prefix trie with cached top K | 60 | core | arena trie, promote an id along its path after a score increase, lookup by walking the query | equals sorting all prefix matches after seeded adds and events, including more than K terms under one prefix | needs: none | 417-472 |
| m6 | Trigram index for infix and typo candidates | 60 | core | sorted posting lists, rarest-first intersection, shared-gram counting | candidates are a superset of every verified match found by scanning all terms | needs: none | 474-519 |
| m7 | Seeded term and query generators with brute-force oracle | 60 | core | Unicode term generator, typo mutator, mixed query generator, oracle that classifies every term and sorts | determinism by seed; hand-checked oracle answers; generated queries cover each kind | needs: none | 634-648,1137-1260 |
| m8 | Two-tier search and ranker | 90 | core | Index applying operations to table, trie and trigrams; prefix hits first, then verified candidates by distance and rank; epoch counter | equals the oracle on seeded terms and queries, with non-vacuity counts | needs: none; the file size (5000 terms, 2000 queries) must shrink for the 15 s debug cap (unverified) | 521-632,1388-1434 |
| m9 | LRU cache with targeted retain | 60 | core | index-linked list with free slots, get, put, capacity zero, retain by predicate | eviction order table; random operations against a Vec model | needs: none | 650-770,1365-1377 |
| m10 | Engine with generation-stamped cache | 90 | core | RwLock store, writer mutex, cache valid only for the current generation, invalidation of only the queries a changed term can match, counters | after each seeded update every hot query equals the oracle; unrelated entries still hit; forced stale install is rejected | needs: none | 781-807,817-951,1444-1499 |
| m11 | Latency histogram and metrics text | 60 | core | lock-free log-linear histogram with quantiles, counters and gauges in text exposition | quantiles within the error bound on 1..10000; golden metric lines with recorded values | needs: none | 102-153,953-1013,1378-1385 |
| m12 | HTTP API with axum | 90 | core | suggest, events, index and metrics routes, serde request types, 404 for unknown term, decayed score in responses, serve with graceful stop | scripted transcript through tower oneshot on a curated dictionary with a manual clock: ranking, cached flag, invalidation, 404, add visible at once | needs: reqwest is not vendored, so use tower oneshot; loopback bind and serve unverified | 1015-1135,1271-1339,1611-1721 |
| m13 | Copy-on-write snapshot store | 45 | optional | second store that clones the index per write and swaps an Arc | same answers and generation as the RwLock store after the same seeded writes; an old snapshot stays unchanged | needs: none | 772-815,1578-1608 |
| m14 | Store comparison under load | 60 | optional | reader and writer threads, throughput and p99 per store, choose one | assert only that both make progress and join cleanly; figures are printed | needs: none; timing figures are not gradable | 1501-1577 |
| m15 | Concurrent clients replayed by generation | 90 | optional | concurrent clients during writes; every response records its generation and is replayed against the oracle at that state | each answer equals the oracle at its reported generation; final generation equals number of writes | needs: reqwest is not vendored (drive the Router with oneshot, or hand-write a client over loopback, unverified) | 1304-1330,1723-1871 |

**Readiness**

| Skill, concept or API | Taught by | Status | First needed |
|---|---|---|---|
| injectable clock trait with a manual atomic implementation | fake-clock (TEST 1) | to be written | m4 |
| Unicode search keys: char to_lowercase expansion, accent folding, combining marks, whitespace collapse | unicode-search-keys (TEXT 6) *prep* | to be written | m1 |
| Damerau-Levenshtein with transpositions, rolling rows and a distance bound | fuzzy (SEARCH 7) | to be written | m2 |
| q-gram count filter for typo candidates: padding, threshold from the edit budget, then verify | qgram-typo-filter (SEARCH 15) *prep* | to be written | m3 |
| forward-decayed popularity that keeps relative order without rescoring | autocomplete (SEARCH 6) | to be written | m4 |
| arena trie with child indices | trie (COLL 6) | staged draft `f4-prefix-trie` (new in build-your-own) | m5 |
| cached top K per trie node maintained on score increases | autocomplete (SEARCH 6) | to be written | m5 |
| trigram inverted index, intersect rarest posting list first, verify substring | trigram (SEARCH 4) | exists: `y10-trigram-index` | m6 |
| seeded xorshift generators for Unicode terms, typos and query mixes | seeded-data-gen (TEST 11) *prep* | to be written | m7 |
| brute-force oracle against the indexed engine with non-vacuity counts | test-differential (TEST 6) | to be written | m7 |
| O(1) LRU as an index-linked list in a Vec, with removal of arbitrary entries | lru (CACHE 1) | staged draft: `f4-lru-cache-arena` | m9 |
| generation-stamped cache: never install a stale answer, invalidate only affected keys, replay answers by generation, lock order | generation-cache (CACHE 9) *prep* | to be written | m10 |
| log-linear latency histogram with atomics, fetch_max and quantiles | atomic-counters (MON 1) | exists: `c3-atomic-stats` | m11 |
| text exposition of counters, gauges and summary quantiles | mon-metrics (MON 2) | to be written (partly practised by `c3-atomic-stats`) | m11 |
| axum Router, State, Query and Json extractors, status codes, testing with tower oneshot | axum-api (WEB 3) | staged draft: `b1-api-error-response` | m12 |
| serde derive with default functions for request bodies | serde-wire (BIN 3) | staged draft: `f7-wire-format-json` | m12 |
| server task with oneshot shutdown signal and a deadline on the join | graceful-shutdown (ASYNC 7) | staged draft: `c4-drain-with-deadline` | m12 |
| Arc snapshot swap against in-place RwLock, serialised writers | rcu-snapshot (ATOM 3) | to be written | m13 |

<details><summary>Suspected bugs and weak spots in the Cookbook file (8 notes, all unverified: found by reading, nothing run)</summary>

- weight uses 2^(now / 3600) as f64: after about 1024 half-lives (about 43 days) it becomes infinite, ranking collapses to tie-breaks and the response score is infinity times zero, NaN (lines 345-349,1051-1055, unverified)
- the shared-gram floor of 2 with padded grams drops real typos in half-typed words: rsut against rustacean has prefix distance 1 but seems to share only one gram; the oracle applies the same rule so tests cannot see it (lines 288-331, unverified)
- normalize filters combining marks before lowercasing, so capital dotted I lowercases to i plus a combining dot that survives; normalize is then not idempotent (lines 181-197, unverified)
- query length and index batch size are not limited by the engine; invalidation runs class_of for every cached key times every changed term while holding the cache lock
- async handlers call apply and suggest directly, taking std locks and possibly cloning the whole index on a runtime worker
- events and index responses read the epoch after apply returns, so under concurrent writers it may belong to a later write
- weak checks: warm.cached OR any earlier hit is nearly always true, and one oracle check passes a literal true (lines 1417,1428-1431)
- the file uses reqwest, which is not in the runner dependency list

</details>


### Cookbook multi-file projects

#### `cap-pebble`: PEBBLE compiler front to back

Compilers · hard · P3 · Cookbook: `cookbook/crates/cb-compiler/`. Existing practice: none. Today: no: 57 files and about 21,500 lines; needs multi-file projects (Courses engine).
Place: after roadmap position 246 of 435, `iv-ssa-regalloc` (LANG 12), in the branch order. Series ending here: none.

**Whole file read: no.** This capstone's readiness is not fully mapped. It is to be mapped properly when its series is built.

**Outline stages** (not split into 45 to 120 minute milestones, because the code was not read in full)

| Stage | Adds | Tested by |
|---|---|---|
| Lexer and recoverable parser | tree and errors | goldens |
| Names and types | resolution and inference | diagnostics |
| Exhaustiveness | pattern checks | tables |
| Tree interpreter | execution | corpus |
| Middle end | SSA and optimisations | differential |
| Bytecode and VM | code generation and verifier | differential and fuzz |

**Provisional readiness** (the parts it is expected to reuse; not a skill-by-skill table)

| Part | Status |
|---|---|
| tokenizer-borrowed (PARSE 2) | exists: `l3-zero-copy-tokenizer` |
| hindley-milner (LANG 9) | to be written |
| bytecode-vm (LANG 5) | to be written (partly practised by `f2-eight-byte-instructions`) |
| ssa-regalloc (LANG 12) | to be written |

#### `cap-pebbletools`: Formatter, highlighter and language server

Developer tools · hard · P3 · Cookbook: `cookbook/crates/cb-compiler-tools/`. Existing practice: none. Today: no: multi-file, depends on the compiler crate, speaks over stdio.
Place: after roadmap position 242 of 435, `iv-formatter` (LANG 8), in the branch order. Series ending here: none.

**Whole file read: no.** This capstone's readiness is not fully mapped. It is to be mapped properly when its series is built.

**Outline stages** (not split into 45 to 120 minute milestones, because the code was not read in full)

| Stage | Adds | Tested by |
|---|---|---|
| Formatter | document algebra | idempotence |
| Highlighter | semantic tokens | goldens |
| Analysis | symbols and definitions | workspace fixtures |
| LSP transport | JSON-RPC framing | scripted sessions |
| LSP features | hover, completion, rename | goldens |

**Provisional readiness** (the parts it is expected to reuse; not a skill-by-skill table)

| Part | Status |
|---|---|
| formatter (LANG 8) | to be written |
| diagnostics-renderer (ERR 1) | to be written |
| length-codec (BIN 2) | exists: `b4-length-prefixed-codec` |

#### `cap-backend`: Multi-tenant SaaS backend

Web services · hard · P2 · Cookbook: `cookbook/crates/cb-backend-reference/`. Existing practice: none. Today: no: 49 files, sockets and a build script; Courses engine.
Place: after roadmap position 298 of 435, `iv-sec-passwords` (SEC 8), in the branch order. Series ending here: SEC.

**Whole file read: no.** This capstone's readiness is not fully mapped. It is to be mapped properly when its series is built.

**Outline stages** (not split into 45 to 120 minute milestones, because the code was not read in full)

| Stage | Adds | Tested by |
|---|---|---|
| Config and errors | one error shape | tables |
| Accounts and tokens | passwords and sessions | attack corpus |
| Authorisation | tenant policy | truth table |
| Resources and pagination | tasks and projects | API tests |
| Idempotency, caching, rate limits | headers | scripted |
| Metrics, faults, shutdown | operations | loopback tests |

**Provisional readiness** (the parts it is expected to reuse; not a skill-by-skill table)

| Part | Status |
|---|---|
| axum-api (WEB 3) | staged draft: `b1-api-error-response` |
| sec-passwords (SEC 8) | to be written |
| authz (SEC 6) | to be written |
| cursor-pagination (WEB 2) | to be written |
| idempotency-store (RESIL 4) | to be written |
| rate-limiter (RESIL 2) | staged draft `m2-rate-limiter` (new in build-your-own) |

#### `cap-vectordb`: Mini vector database

Vector search · hard · P2 · Cookbook: similar `mini-vector-database`. Existing practice: none. Today: core stages yes with in-memory files; size (3,756 lines) argues for a multi-file project.
Place: after roadmap position 339 of 435, `iv-hnsw` (SEARCH 14), in the branch order. Series ending here: none.

**Whole file read: no.** This capstone's readiness is not fully mapped. It is to be mapped properly when its series is built.

**Outline stages** (not split into 45 to 120 minute milestones, because the code was not read in full)

| Stage | Adds | Tested by |
|---|---|---|
| Schema and values | typed records | tables |
| Filter language | parse and evaluate | oracle |
| Brute-force search | exact top-k | oracle |
| Graph segment | HNSW-lite | recall floor |
| Quantisation | compress and rescore | recall floor |
| WAL and snapshots | recovery | crash model |
| Segments and compaction | memtable to immutable | model |
| Concurrency and protocol | readers during writes | stress |

**Provisional readiness** (the parts it is expected to reuse; not a skill-by-skill table)

| Part | Status |
|---|---|
| knn (SEARCH 9) | to be written |
| hnsw (SEARCH 14) | to be written |
| boolean-query (SEARCH 3) | to be written |
| wal (STORE 2) | exists: `y9-kv-store-wal` |
| compaction (STORE 6) | staged draft: `s6-merge-sorted-streams` |


### Composed from several Cookbook pieces (the Cookbook has no single page that is this project)

#### `cap-database`: Build your own database

Databases · hard · P1 · Cookbook: similar `slotted-pages-heap-file`, similar `buffer-pool-clock`, similar `bplus-tree`, similar `wal-append-log`, similar `aries-recovery`, similar `tiny-sql-engine`, similar `mvcc-snapshot-isolation`. Existing practice: f2-slotted-page, y9-kv-store-wal, BusTub course. Today: early stages yes; the whole thing is multi-file (Courses; the BusTub course already covers much of it).
Place: after roadmap position 325 of 435, `iv-timestamp-ordering` (TXN 6), in the branch order. Series ending here: TXN.

**Whole file read: no.** This capstone's readiness is not fully mapped. It is to be mapped properly when its series is built.

**Outline stages** (not split into 45 to 120 minute milestones, because the code was not read in full)

| Stage | Adds | Tested by |
|---|---|---|
| Pages and heap file | records | model |
| Buffer pool | pins and eviction | trace checks |
| B+ tree index | search and insert | oracle |
| SQL front end | parse and bind | goldens |
| Executor | operators and joins | naive oracle |
| Logging and recovery | crash safety | crash at every point |
| Transactions | snapshot isolation | serialisability checker |

**Provisional readiness** (the parts it is expected to reuse; not a skill-by-skill table)

| Part | Status |
|---|---|
| slotted-page (STORE 3) | exists: `f2-slotted-page` |
| heap-file (STORE 4) | to be written |
| buffer-pool (STORE 9) | to be written |
| disk-bptree (STORE 13) | to be written |
| sql-parser (QUERY 7) | to be written |
| volcano (QUERY 2) | to be written |
| checkpoint-recovery (STORE 14) | to be written |
| mvcc-si (TXN 3) | staged draft: `y12-mvcc-snapshot-isolation` |

#### `cap-gitstore`: Git-like object store

Data and history · medium · P2 · Cookbook: similar `merkle-tree`, similar `dp-lcs-diff`, similar `compression-lz77-huffman`. Existing practice: y8-shortest-edit-script. Today: yes in memory (sha2 vendored; Git's own SHA-1 format and zlib are not covered).
Place: after roadmap position 375 of 435, `iv-lin-graph` (LIN 7), in the branch order. Series ending here: none.

**Whole file read: no.** This capstone's readiness is not fully mapped. It is to be mapped properly when its series is built.

**Outline stages** (not split into 45 to 120 minute milestones, because the code was not read in full)

| Stage | Adds | Tested by |
|---|---|---|
| Objects | content-addressed blobs | hash round trip |
| Trees and commits | snapshots | checkout equality |
| Refs and branches | moving names | scripts |
| Diff | tree and line diff | apply equals target |
| History | log and merge base | brute force |
| Three-way merge | conflicts | scenarios |
| Packing | delta storage | round trip and size |

**Provisional readiness** (the parts it is expected to reuse; not a skill-by-skill table)

| Part | Status |
|---|---|
| merkle (LIN 2) | to be written |
| git-objects (LIN 3) | to be written |
| diff (LIN 1) | exists: `y8-shortest-edit-script` |
| lin-graph (LIN 7) | to be written |
| lz77 (BIN 8) | to be written |

#### `cap-shell`: Unix shell

Operating systems · hard · P3 · Cookbook: similar `std-process-command-pipelines`, similar `process-pipes-signals`, similar `std-signals-env-exit-hooks-ctrlc`. Existing practice: y9-shell-line-parser. Today: only parsing and expansion today; execution needs processes and binaries in the sandbox.
Place: after roadmap position 265 of 435, `iv-process-pipeline` (IO 7), in the branch order. Series ending here: none.

**Whole file read: no.** This capstone's readiness is not fully mapped. It is to be mapped properly when its series is built.

**Outline stages** (not split into 45 to 120 minute milestones, because the code was not read in full)

| Stage | Adds | Tested by |
|---|---|---|
| Line parser | quotes and pipes | tables |
| Builtins and environment | cd, export | fake OS |
| Running commands | spawn and status | needs processes |
| Pipelines and redirects | fds | needs processes |
| Signals | interrupt foreground only | needs processes |
| Scripts | sequencing and conditionals | corpus with a fake executor |

**Provisional readiness** (the parts it is expected to reuse; not a skill-by-skill table)

| Part | Status |
|---|---|
| shell-line (PARSE 8) | exists: `y9-shell-line-parser` |
| process-pipeline (IO 7) | to be written |
| tree-interpreter (LANG 4) | to be written |

#### `cap-loadbalancer`: Load balancer

Networking · hard · P2 · Cookbook: similar `reverse-proxy-load-balancer`, similar `api-gateway-service-discovery`, similar `consistent-hashing`, similar `http1-parser`. Existing practice: b4-http1-request-parser. Today: policy stages yes; proxy stages need loopback.
Place: after roadmap position 420 of 435, `iv-log-linear-histogram` (MON 9), in the branch order. Series ending here: MON.

**Whole file read: no.** This capstone's readiness is not fully mapped. It is to be mapped properly when its series is built.

**Outline stages** (not split into 45 to 120 minute milestones, because the code was not read in full)

| Stage | Adds | Tested by |
|---|---|---|
| Strategies | pick a backend | balance bounds |
| Health | checks and ejection | fake clock |
| Circuit breaking and retry budget | protection | scripts |
| Request forwarding | header rules | parser goldens |
| Proxy loop | streaming both ways | loopback |
| Drain and metrics | deploys | counts |

**Provisional readiness** (the parts it is expected to reuse; not a skill-by-skill table)

| Part | Status |
|---|---|
| load-balancer (RESIL 5) | to be written |
| circuit-breaker (RESIL 3) | to be written |
| http-request (NET 6) | exists: `b4-http1-request-parser` |
| mon-metrics (MON 2) | to be written (partly practised by `c3-atomic-stats`) |

#### `cap-queue`: Message queue broker

Distributed · hard · P1 · Cookbook: similar `kafka-log-model`, similar `record-batch-codec`, similar `wal-append-log`, similar `systems-design-in-code`, similar `tcp-framed-server`. Existing practice: none. Today: stages 1 to 6 yes in memory; network stage needs loopback.
Place: after roadmap position 261 of 435, `iv-png-container` (BIN 13), in the branch order. Series ending here: BIN.

**Whole file read: no.** This capstone's readiness is not fully mapped. It is to be mapped properly when its series is built.

**Outline stages** (not split into 45 to 120 minute milestones, because the code was not read in full)

| Stage | Adds | Tested by |
|---|---|---|
| Record codec | batches and CRC | round trip |
| Segmented log | offsets and retention | model |
| Topics and partitions | routing | ordering |
| Producers | idempotence | duplicates dropped |
| Consumer groups | assignment and offsets | coverage |
| Acknowledged queue mode | visibility and dead letters | fake clock |
| Replication model | high watermark | crash schedules |
| Wire protocol | framed requests | loopback |

**Provisional readiness** (the parts it is expected to reuse; not a skill-by-skill table)

| Part | Status |
|---|---|
| varint-codec (BIN 1) | to be written (partly practised by `f5-varint-codec`) |
| wal (STORE 2) | exists: `y9-kv-store-wal` |
| partitioned-log (DIST 12) | to be written |
| visibility-queue (DIST 3) | to be written |
| dist-delivery (DIST 2) | to be written |
| length-codec (BIN 2) | exists: `b4-length-prefixed-codec` |

#### `cap-raftkv`: Raft key-value store

Distributed · hard · P1 · Cookbook: similar `raft-consensus`, similar `deterministic-simulation-testing`, similar `linearizability-checker`, similar `mini-kv-store-code`. Existing practice: y7-raft-vote-rules, y7-raft-append-entries, y7-raft-leader-commit. Today: yes on a simulator; large for one lib.rs.
Place: after roadmap position 359 of 435, `iv-raft-reads` (DIST 23), in the branch order. Series ending here: DIST.

**Whole file read: no.** This capstone's readiness is not fully mapped. It is to be mapped properly when its series is built.

**Outline stages** (not split into 45 to 120 minute milestones, because the code was not read in full)

| Stage | Adds | Tested by |
|---|---|---|
| Simulator | network, disk, clock | determinism |
| Election | one leader per term | seeds |
| Replication | log matching | checker |
| Apply and client dedup | KV state machine | same history everywhere |
| Persistence | crash safety | fault injection |
| Snapshots | compaction | lagging follower |
| Linearizable reads | client view | history checker |
| Chaos and shrink | find planted bugs | replay |

**Provisional readiness** (the parts it is expected to reuse; not a skill-by-skill table)

| Part | Status |
|---|---|
| sim-network (DIST 1) | to be written |
| raft-election (DIST 18) | to be written (partly practised by `y7-raft-vote-rules`) |
| raft-replication (DIST 19) | to be written (partly practised by `y7-raft-append-entries`) |
| raft-apply (DIST 20) | to be written (partly practised by `y7-raft-leader-commit`) |
| raft-persist (DIST 21) | to be written |
| raft-snapshot (DIST 22) | to be written |
| raft-reads (DIST 23) | to be written |
| linearizability (DIST 16) | to be written |

#### `cap-jobsched`: Job scheduler without containers

Systems and platforms · hard · P1 · Cookbook: similar `systems-design-in-code`, similar `timing-wheel`, similar `scheduler-policies`, similar `lease-fencing-sim`, similar `saga-orchestrator`, similar `wasm-binary-interpreter`. Existing practice: c2-thread-pool. Today: yes with a fake clock and fake workers; real isolation cannot be graded.
Place: after roadmap position 418 of 435, `iv-alg-gr-huffman` (GRD 3), in the branch order. Series ending here: CONC, SBX, GRD.

**Whole file read: no.** This capstone's readiness is not fully mapped. It is to be mapped properly when its series is built.

**Outline stages** (not split into 45 to 120 minute milestones, because the code was not read in full)

| Stage | Adds | Tested by |
|---|---|---|
| Queue and priorities | delayed jobs | oracle |
| Workers and leases | heartbeats and fencing | pause schedules |
| Retries and dead letters | backoff | fake clock |
| Dependencies | ready-set tracking | topological oracle |
| Quotas and fairness | per-tenant limits | fairness bound |
| Durable log | restart recovery | crash at every point |
| Sandboxed steps | metered bytecode | fuel limits |

**Provisional readiness** (the parts it is expected to reuse; not a skill-by-skill table)

| Part | Status |
|---|---|
| job-scheduler (CONC 15) | to be written |
| lease-fencing (DIST 7) | to be written |
| retry-backoff (RESIL 1) | to be written |
| alg-order (GRAPH 4) | to be written |
| sbx-quota (SBX 3) | to be written |
| sbx-fuel (SBX 5) | to be written (partly practised by `y6-wasm-stack-interpreter`) |
| wal (STORE 2) | exists: `y9-kv-store-wal` |

#### `cap-interp`: Compiler to bytecode for your own language

Languages · hard · P2 · Cookbook: similar `bytecode-vm`, similar `pratt-parser`, similar `hindley-milner-inference`, similar `garbage-collectors`, similar `toy-language-interpreter`. Existing practice: y6-wasm-stack-interpreter. Today: yes up to a size limit.
Place: after roadmap position 239 of 435, `iv-bytecode-vm` (LANG 5), in the branch order. Series ending here: MEM.

**Whole file read: no.** This capstone's readiness is not fully mapped. It is to be mapped properly when its series is built.

**Outline stages** (not split into 45 to 120 minute milestones, because the code was not read in full)

| Stage | Adds | Tested by |
|---|---|---|
| Lexer and parser | errors | goldens |
| Type check | static errors | corpus |
| Bytecode | compile and verify | differential |
| Calls and closures | frames | corpus |
| Heap and collector | mark-sweep | stress with a tiny heap |
| Optimiser | folding and dead code | differential |

**Provisional readiness** (the parts it is expected to reuse; not a skill-by-skill table)

| Part | Status |
|---|---|
| tokenizer-borrowed (PARSE 2) | exists: `l3-zero-copy-tokenizer` |
| pratt (PARSE 12) | staged draft: `y14-pratt-parser` |
| typecheck-simple (LANG 3) | to be written |
| bytecode-vm (LANG 5) | to be written (partly practised by `f2-eight-byte-instructions`) |
| mark-sweep (MEM 5) | to be written |
| constant-fold (LANG 2) | to be written |

#### `cap-collab`: Collaborative block editor core

Collaboration · hard · P2 · Cookbook: similar `crdt-block-editor-notion-like`, similar `crdt-fugue-text-editor`, similar `undo-redo-editors`, similar `crdt-todo-app-no-db`. Existing practice: y9-ot-collaborative-text, y13-or-set-crdt (draft). Today: yes (replicas in memory).
Place: after roadmap position 368 of 435, `iv-ot` (CRDT 15), in the branch order. Series ending here: CRDT.

**Whole file read: no.** This capstone's readiness is not fully mapped. It is to be mapped properly when its series is built.

**Outline stages** (not split into 45 to 120 minute milestones, because the code was not read in full)

| Stage | Adds | Tested by |
|---|---|---|
| Replica and delivery | causal buffer | random orders |
| Text sequence | concurrent typing | convergence and no interleaving |
| Block tree | moves without cycles | invariants |
| Formatting marks | concurrent ranges | scenarios |
| Undo | own operations only | scripts |
| Sync | deltas and snapshots | bytes bound |

**Provisional readiness** (the parts it is expected to reuse; not a skill-by-skill table)

| Part | Status |
|---|---|
| crdt-opbased (CRDT 9) | to be written |
| text-crdt (CRDT 11) | to be written |
| crdt-text-fugue (CRDT 12) | to be written |
| crdt-movable-tree (CRDT 14) | to be written |
| crdt-undo (CRDT 13) | to be written |
| crdt-delta (CRDT 8) | to be written |

#### `cap-lineage`: Workflow lineage and recompute engine

Data lineage · hard · P2 · Cookbook: similar `event-sourced-ledger`, similar `datalog-engine-semi-naive`, similar `incremental-view-maintenance-dbsp`, similar `delta-lake-transaction-log`. Existing practice: none. Today: yes; the Cookbook has no page about lineage itself.
Place: after roadmap position 410 of 435, `iv-alg-flow` (GRAPH 7), in the branch order. Series ending here: LIN, GRAPH.

**Whole file read: no.** This capstone's readiness is not fully mapped. It is to be mapped properly when its series is built.

**Outline stages** (not split into 45 to 120 minute milestones, because the code was not read in full)

| Stage | Adds | Tested by |
|---|---|---|
| Event log of runs | facts | replay |
| Dataset version graph | upstream and downstream | reachability oracle |
| As-of queries | past graphs | replay oracle |
| Column lineage | through plans | perturbation check |
| Staleness and recompute | minimal rebuild | build-all oracle |
| Table log with snapshots | optimistic commits | conflicts |

**Provisional readiness** (the parts it is expected to reuse; not a skill-by-skill table)

| Part | Status |
|---|---|
| lin-event-store (LIN 4) | to be written |
| lin-graph (LIN 7) | to be written |
| lin-column (LIN 9) | to be written |
| lin-recompute (LIN 8) | to be written |
| lin-projections (LIN 5) | to be written |

#### `cap-tsdb`: Time-series database

Databases · hard · P3 · Cookbook: similar `tsm-time-series-engine`, similar `timeseries-gorilla`, similar `hypertables-continuous-aggregates`. Existing practice: none. Today: core yes in memory.
Place: after roadmap position 415 of 435, `iv-alg-iv-queries` (IVL 4), in the branch order. Series ending here: IVL.

**Whole file read: no.** This capstone's readiness is not fully mapped. It is to be mapped properly when its series is built.

**Outline stages** (not split into 45 to 120 minute milestones, because the code was not read in full)

| Stage | Adds | Tested by |
|---|---|---|
| Chunk encoding | compression | bit-exact round trip |
| Head and WAL | ingest | recovery |
| Blocks and compaction | immutable data | model |
| Label index | matchers and postings | scan oracle |
| Retention and downsampling | rollups | recompute oracle |

**Provisional readiness** (the parts it is expected to reuse; not a skill-by-skill table)

| Part | Status |
|---|---|
| gorilla (BIN 7) | to be written |
| wal (STORE 2) | exists: `y9-kv-store-wal` |
| inverted-index (SEARCH 1) | to be written (partly practised by `y10-trigram-index`) |
| compaction (STORE 6) | staged draft: `s6-merge-sorted-streams` |
| asof-join (QUERY 11) | to be written |

#### `cap-unsafekit`: Your own unsafe containers crate

Unsafe Rust · hard · P2 · Cookbook: similar `maybeuninit-fixedvec`, similar `nonnull-linked-list`, similar `bump-arena-allocator`, similar `spsc-ring-buffer`, similar `own-arc-refcount`. Existing practice: y2-array-vec, y2-mini-vec, y2-unsafe-linked-list, c3-mini-arc, c3-spsc-ring. Today: yes for behaviour; soundness needs Miri and ordering needs loom, neither available.
Place: after roadmap position 217 of 435, `iv-interleaving-checker` (ATOM 9), in the branch order. Series ending here: UNSAFE, ATOM.

**Whole file read: no.** This capstone's readiness is not fully mapped. It is to be mapped properly when its series is built.

**Outline stages** (not split into 45 to 120 minute milestones, because the code was not read in full)

| Stage | Adds | Tested by |
|---|---|---|
| Inline vector | MaybeUninit storage | drop counting |
| Growable vector | raw allocation | differential against Vec |
| Linked deque | NonNull links | model tests |
| Arena | bump allocation | overlap and drop checks |
| Shared pointer | atomic counts | stress and drop counting |
| SPSC channel | ordering | sequence and drop checks |

**Provisional readiness** (the parts it is expected to reuse; not a skill-by-skill table)

| Part | Status |
|---|---|
| fixed-vec (UNSAFE 4) | exists: `y2-array-vec` |
| myvec (UNSAFE 5) | exists: `y2-mini-vec` |
| intrusive-list (UNSAFE 6) | exists: `y2-unsafe-linked-list` |
| bump-arena (MEM 2) | to be written (partly practised by `f3-typed-arena`) |
| arc (ATOM 4) | to be written (partly practised by `c3-mini-arc`) |
| spsc (ATOM 5) | exists: `c3-spsc-ring` |

**A capstone-readiness gap that is not closed:** the three multi-file projects and all composed capstones lack a readiness table built from a full read. Before building one of them, do that reading first and add whatever parts it shows to be missing.

## 8. What today's runner cannot grade

Read in `crates/runner` on 2026-11-26: one `lib.rs`, edition **2021**, `cargo test` with a 90 s build and 15 s test limit, Docker with no network, a read-only root, a tmpfs and capped memory, CPU and processes. Crates only from `docker/deps/Cargo.toml`. Doctests are off.

| Not gradable | Affects | What it would take |
|---|---|---|
| Miri (undefined behaviour) | unsafe parts (UNSAFE, SMART, MEM, ATOM) | a nightly toolchain with Miri in the image and a runner step |
| loom (memory orderings) | ATOM, SPSC, MPMC, Arc | vendor `loom`; problems written against a `sync` facade |
| Compile-fail expectations (typestate, units, sealed traits) | TRAIT, SEC, SBX | `trybuild` or a compile-expectation step in the runner; today only fix-mode rules and positive tests |
| Procedural macros | derive-builder | multi-file submissions (a proc-macro crate) and `syn`, `quote` |
| Features, `cfg`, workspaces, build scripts, cdylib | L9 topics, `cb-features-workspace`, `cb-build-script`, `cb-cdylib-c-abi` | multi-file projects and a build matrix (Courses engine) |
| Proof of `no_std` | nostd-ring | a rule or a no_std test target |
| cargo-fuzz, criterion, flamegraphs | fuzzing, benchmarking | nightly and tooling; today only random-input tests and `[perf]` |
| `mio`, `libc`, `memmap2` | event-loop, mio-reactor-api, runtime reactor milestones, signals, mmap | vendor them |
| `reqwest`, hyper client; `futures-util` and `http-body-util` as direct crates | typeahead and job-service ports | vendor, or port to `tower::ServiceExt::oneshot`, `futures::stream`, `axum::body::to_bytes` |
| Databases (sqlx, rusqlite) and external services | B3, outbox with SQL | database crates and, for Postgres, a service |
| Loopback sockets, writable temp files, spawning processes | NET, IO, shell, several capstone milestones | **not verified**: probably possible inside the container; one probe problem in the Docker sandbox per case would settle it |
| Multi-file or very large builds | PEBBLE, backend reference, large capstones | multi-file submissions or the Courses engine |
| Namespaces, seccomp, containers | real sandboxing | out of reach, and the Cookbook has no recipe |
| Picking tests to run (core only, one part) | every multi-part problem | a runner filter or a per-part status |

Also: reference code copied from the Cookbook may use edition 2024 syntax (mini Redis uses `let` chains) and must be ported to 2021. Several Cookbook test suites are likely too slow for a 15 s debug build (not measured).

## 9. The six written problems

Staged in the FlowForge folder `anneal-staging/build-your-own/` (see its `README.md` and `cookbook-refs.toml`). The earlier 20 are in `anneal-staging/rust-practice/`.

| Problem id | Series part | Track | Time box | Parts before it in its series (all unwritten unless marked) |
|---|---|---|---|---|
| `s5-binary-heap` | COLL 4 | S5 Queues & heaps (new track, code and name from CURRICULUM) | 60 | minstack-queue, ring-deque, hashmap-chaining |
| `s5-indexed-priority-queue` | COLL 8 | S5 | 90 in the series (the statement says 60) | COLL 1-3, 5-7: median-stream, trie (**written**), union-find (**written**) |
| `f4-union-find` | COLL 7 | F4 (addition, new stage `query-structures`) | 60 | COLL 1-3, 5; binary-heap and trie are written |
| `f4-prefix-trie` | COLL 6 | F4 (addition) | 60 | COLL 1-3, 5; binary-heap written |
| `f4-range-queries` | COLL 13 | F4 (addition) | 120 | COLL 9-12 (indexed-pq at 8 is written) |
| `m2-rate-limiter` | RESIL 2 | M2 Machine coding (new track, code and name from CURRICULUM) | 120 | retry-backoff; also its Required part fake-clock (TEST 1) |

Series numbers here are those of section 5. None of the earlier parts is needed to attempt these: each problem has its own starter. One mismatch to settle on integration: the indexed queue's statement says 60 minutes, and the series plan gives it 90; its exact box should follow whichever the owner finds true.

**Time boxes and timers.** `problem.toml` has no time field and rejects unknown fields. The workspace clock and the progress statistics derive the budget from mode and level (`budget` in `crates/api/src/progress.rs` and `BUDGET` in `Workspace.tsx`: write 10, 25, 40; fix 5, 10, 15; stage 60, 60, 90). No setting gives 120. So the box is the first line of each statement and a tag (`60-min`, `120-min`), and the clock will show 25 or 40. To fix that: add `time_box_minutes: Option<u32>` to `ProblemFile`, send it in the problem view, and use it in both places, which also changes the "within budget" statistic. `mode = "stage"` was not used: it means "only tests are given".

**Core before follow-ups.** The runner cannot run part of a suite or mark a part passed. What works today: every follow-up signature is in the starter with `todo!()`, so the code compiles; test names start `core_`, `part2_`, `part3_`, `part4_`; Run shows the visible tests of all parts and Submit all tests. With only the core written, all `core_` tests pass and later ones fail. A problem counts as solved only when everything passes. Tested follow-ups are in the statement; the `follow_up` field holds discussion questions only. That field is shown before solving (RUST_PRACTICE.md weakness W1).

## 10. Series in the app

**What the loader supports today.** A track (code, name, section, tier, stages whose bands may not go down) and problems with `stage` and a unique `order`. A problem id is the track code plus the slug. There is no series, no prerequisite field, no hidden link, and `related` holds only track codes.

**A series should not be a track.** A series mixes subjects and existing problems. Making it a track would move folders, which changes ids; saved progress is keyed by them, so that must not happen. Use **new metadata that only points at ids**:

```
content/series/<key>.toml
key, title, kind, goal, capstone
[[part]] problem = "<existing or new id>", minutes, idea, use, required = [..], helpful = [..], hidden = [..]
```

New problems still live in a track (S5, F4, M2, C1 and so on, following `docs/CURRICULUM.md`; many parts here have no obvious track yet, which is a decision for each series when it is built). A series can list an existing problem as one of its parts, for example `c1-blocking-queue` as CONC's queue.

**Needed:** a model type and loader (I did not check whether the loader ignores an unknown `content/series` folder); validation that ids exist, each problem has one home series, `required` has no cycle and points backwards in the roadmap; an endpoint; a series page with "next part", closed disclosures for hidden links, and the concept map. No database migration: progress stays per problem. It is a UI change, so it needs a mockup first. The web-app notes about new tracks and section B in the earlier batch's README apply to S5 (`PLANNED` in `curriculum.ts`) and M2 (section M is in the loader's `Section` enum; I did not check that the web app shows it; see W6 and W7 in RUST_PRACTICE.md).

**Carrying your own code forward** (RUST_PRACTICE.md section 7.3). Today every part starts from its own starter, so skipping one blocks nothing. To offer "start from my passing previous part": a field naming the earlier problem id whose code carries over, an endpoint returning its last passing submit, a button that writes only the new draft and first copies any existing draft to `data_archive` with confirmation, and a verify step that each part's solution passes the earlier parts' tests too. Parts must then share an API that only grows. No migration. It helps most where one program grows: **Raft in DIST, the storage engine (STORE), LANG, ASYNC, the CRDT replica, and the capstones**. It helps little in COLL, PARSE's small formats or the algorithm series, where each part is a new program.

## 11. FlowForge: its own path

FlowForge, the workflow engine the owner builds separately, is a capstone path of its own. Its series plan is `SERIES.md` at the root of the flowforge repository. This catalogue writes no exercises or solutions for it. Ideas practised here (job scheduling, topological order, persistence, services) also appear there.

**Series are built one at a time** (owner's decision). A separate piece of work writes the web server series (`anneal-staging/series/web-server/` in the FlowForge folder); the NET and WEB parts here are catalogue entries only and may be superseded by it. When a series is built, redo its capstone's readiness mapping then.

