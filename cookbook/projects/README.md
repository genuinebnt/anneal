# Projects: use what the recipes teach

The recipes show an idea once. Projects make you use it five times in a row in a slightly different shape, and that repetition is what makes it stick. Each project below is a spec (goals, milestones, acceptance tests, data to generate, design questions), never a solution: you write the code, your tests decide when a milestone is done.

## How to pick

1. **By time.** Take the size from the legend below and pick one that fits your next free block.
2. **By gap.** Open `HOW_TO_STUDY.md` -> pick the track you just finished (`paths/tracks/`) -> choose a project whose "Prerequisite tracks" column is covered by it.
3. **By role.** Backend: 07, 03, 04. Systems: 02, 08, 03. Language tooling: 05, 06. Product/collab: 09, 07. Portfolio: 10 (after at least 3 smaller projects it reuses).
4. **Ladder, not leap.** Do one `S` in a theme before the `M`, one `M` before the `L`. The project text names the earlier projects it builds on.
5. **Katas** (10 per theme file, 30 minutes each) are warm-ups or follow-ups: do two the day after finishing a recipe to prove you can reproduce the idea without looking.

## Difficulty legend

| Size | Time | Shape |
|---|---|---|
| S | 2-4 hours | one concept, one file or crate, 3-5 milestones |
| M | a weekend | a few modules, a test harness with an oracle, a write-up |
| L | 1-2 weeks | a real component, benchmarks, fault injection or model checking |
| XL | 1 month or more | a system; only in `10_capstone_portfolio.md` |

Prerequisite track codes: T1 Rust foundations, T2 memory and unsafe, T3 concurrency and async, T4 data structures and algorithms, T5 performance, T6 databases (CMU), T7 distributed systems, T8 compilers, T9 networking and web, T10 search and vectors, T11 OS and storage (all in `../paths/tracks/`).

## The working method for every project

- **Spec -> oracle -> test -> code.** Write the oracle (naive, obviously correct) and the acceptance test from the project page before the real implementation.
- **Seeded randomness only.** Every generator takes a seed; every failing assertion prints it.
- **One `NOTES.md` per project** answering the "design questions" in a short write-up (half a page is enough). If you cannot answer one, that is the next thing to study.
- **Measure, do not believe.** Any number in a spec ("under 1 ms", "recall >= 0.95") is a target you verify and report, with the machine.
- **Mentor review.** When a milestone passes, ask for a review: correctness first, then missed edge cases, then idiom, then performance.
- 🔜 next to a recipe stem means the recipe is planned but not written; use the linked doc or source instead.
- All external links on these pages were checked to return HTTP 200 when written.

## All projects (74)

| ID | Project | Size | Themes | Prerequisite tracks | File |
|---|---|---|---|---|---|
| 1.1 | `lsx`: a directory lister with excellent errors | S | cli, errors | T1, T2 | [01](01_rust_craft.md) |
| 1.2 | `ids`: a newtype-ID and typed-handle library | S | newtypes, arenas | T1, T2 | [01](01_rust_craft.md) |
| 1.3 | `confcheck`: layered config with span diagnostics | M | serde, diagnostics | T1, T2 | [01](01_rust_craft.md) |
| 1.4 | `wirefmt`: a typestate protocol client | M | typestate, sans-io | T1, T2 | [01](01_rust_craft.md) |
| 1.5 | `evbus`: a typed plugin/event bus | M | trait objects, type erasure | T1, T2 | [01](01_rust_craft.md) |
| 1.6 | `minstd`: rebuild `Vec`, `Rc`/`Arc`, `HashMap` | L | unsafe, atomics, Miri | T1, T2 | [01](01_rust_craft.md) |
| 1.7 | `crate-polish`: make a library publishable | L | API design, CI, docs | T1, T2 | [01](01_rust_craft.md) |
| 2.1 | `boundq`: a bounded blocking queue with a stress harness | S | condvar, testing | T1, T3, T11 | [02](02_systems_and_concurrency.md) |
| 2.2 | `shardmap`: a sharded concurrent map vs `RwLock<HashMap>` | S | sharding, benchmarks | T1, T3, T11 | [02](02_systems_and_concurrency.md) |
| 2.3 | `poolrs`: a work-stealing thread pool with scoped tasks | M | work stealing, scoped threads | T1, T3, T11 | [02](02_systems_and_concurrency.md) |
| 2.4 | `cronq`: a job scheduler with retries and a timing wheel | M | scheduler, timing wheel, retries | T1, T3, T11 | [02](02_systems_and_concurrency.md) |
| 2.5 | `logpipe`: a log pipeline with backpressure | M | log pipeline, backpressure, tokio | T1, T3, T11 | [02](02_systems_and_concurrency.md) |
| 2.6 | `rsh`: a tiny Unix shell | M | processes, pipes, signals | T1, T3, T11 | [02](02_systems_and_concurrency.md) |
| 2.7 | `miniasync`: your own executor, timers and TCP echo | L | async runtime, epoll/kqueue | T1, T3, T11 | [02](02_systems_and_concurrency.md) |
| 2.8 | `lfq`: lock-free queues verified with loom | L | lock-free, loom | T1, T3, T11 | [02](02_systems_and_concurrency.md) |
| 3.1 | `walog`: a checksummed write-ahead log | S | WAL, crash recovery | T4, T6, T11 | [03](03_databases_and_storage.md) |
| 3.2 | `sstkit`: a sorted-string-table file with a Bloom filter | S | SSTable, Bloom filter | T4, T6, T11 | [03](03_databases_and_storage.md) |
| 3.3 | `lsmdb`: an LSM key-value store | M | LSM, compaction | T4, T6, T11 | [03](03_databases_and_storage.md) |
| 3.4 | `pagedb`: a disk B+tree with a buffer pool | M | B+tree, buffer pool | T4, T6, T11 | [03](03_databases_and_storage.md) |
| 3.5 | `mvccdb`: snapshot isolation key-value store | M | MVCC, SI | T4, T6, T11 | [03](03_databases_and_storage.md) |
| 3.6 | `sqlmini`: a SQL subset over CSV | M | SQL, parser, joins | T4, T6, T11 | [03](03_databases_and_storage.md) |
| 3.7 | `colfile`: a columnar file format with vectorized scans | L | columnar, vectorized | T4, T6, T11 | [03](03_databases_and_storage.md) |
| 3.8 | `optjoin`: a cost-based join-order optimizer | L | optimizer, statistics | T4, T6, T11 | [03](03_databases_and_storage.md) |
| 4.1 | `resilient`: retry, backoff, hedging and circuit breaker | S | retries, circuit breaker | T3, T7 | [04](04_distributed_systems.md) |
| 4.2 | `ringkv`: a consistent-hash sharded KV router | M | sharding, consistent hashing | T3, T7 | [04](04_distributed_systems.md) |
| 4.3 | `minikafka`: a partitioned log with consumer groups | M | mini-Kafka, logs | T3, T7 | [04](04_distributed_systems.md) |
| 4.4 | `lockd`: a lease-based lock service with fencing tokens | M | leases, fencing | T3, T7 | [04](04_distributed_systems.md) |
| 4.5 | `raftlab`: Raft leader election and log replication in a simulator | L | Raft, simulation | T3, T7 | [04](04_distributed_systems.md) |
| 4.6 | `dynamo`: a quorum KV with vector clocks and read repair | M | quorums, vector clocks | T3, T7 | [04](04_distributed_systems.md) |
| 4.7 | `swimmer`: gossip membership and failure detection | L | gossip, failure detection | T3, T7 | [04](04_distributed_systems.md) |
| 5.1 | `calc`: a calculator with a Pratt parser and error recovery | S | Pratt parser, diagnostics | T1, T8 | [05](05_compilers_and_languages.md) |
| 5.2 | `tomlite`: a config-language parser with spans and good diagnostics | S | parsing, spans | T1, T8 | [05](05_compilers_and_languages.md) |
| 5.3 | `loxrs`: a tree-walking interpreter | M | interpreter, closures | T1, T8 | [05](05_compilers_and_languages.md) |
| 5.4 | `loxvm`: a bytecode compiler and stack VM | M | bytecode VM | T1, T8 | [05](05_compilers_and_languages.md) |
| 5.5 | `hmlite`: a type checker with Hindley-Milner inference | M | type inference | T1, T8 | [05](05_compilers_and_languages.md) |
| 5.6 | `rx`: a regex engine from NFA to lazy DFA | M | regex, automata | T1, T8 | [05](05_compilers_and_languages.md) |
| 5.7 | `lspd`: a language server for your language | L | LSP, incremental | T1, T8 | [05](05_compilers_and_languages.md) |
| 5.8 | `miniir`: SSA IR with optimizations and register allocation | L | SSA, regalloc | T1, T8 | [05](05_compilers_and_languages.md) |
| 6.1 | `typeahead`: a prefix autocomplete service | S | trie, autocomplete | T4, T10 | [06](06_search_vectors_and_data.md) |
| 6.2 | `sketchd`: approximate-stats sidecar (Bloom, Count-Min, HyperLogLog) | S | sketches | T4, T10 | [06](06_search_vectors_and_data.md) |
| 6.3 | `trigrep`: a trigram index for code search | M | trigram index | T4, T10 | [06](06_search_vectors_and_data.md) |
| 6.4 | `bm25d`: a full-text search engine with ranking | M | BM25, inverted index | T4, T10 | [06](06_search_vectors_and_data.md) |
| 6.5 | `knnbench`: exact kNN, quantization and a recall harness | M | kNN, quantization, SIMD | T4, T10 | [06](06_search_vectors_and_data.md) |
| 6.6 | `hnsw`: an HNSW index tuned for recall/latency | L | HNSW, ANN | T4, T10 | [06](06_search_vectors_and_data.md) |
| 6.7 | `dedupe`: near-duplicate detection with MinHash LSH | M | MinHash, LSH | T4, T10 | [06](06_search_vectors_and_data.md) |
| 6.8 | `hybrid`: hybrid search with filters and reciprocal rank fusion | L | hybrid search, filters | T4, T10 | [06](06_search_vectors_and_data.md) |
| 7.1 | `httpd`: an HTTP/1.1 parser and static file server | S | HTTP, parsing, security | T3, T9 | [07](07_networking_and_web.md) |
| 7.2 | `shortly`: a URL shortener with sharded storage | M | URL shortener, sharding, cache | T3, T9 | [07](07_networking_and_web.md) |
| 7.3 | `jobapi`: a REST service with idempotency, pagination and rate limiting | M | REST, idempotency, rate limit | T3, T9 | [07](07_networking_and_web.md) |
| 7.4 | `limitd`: a rate-limiter service with five algorithms | M | rate limiting algorithms | T3, T9 | [07](07_networking_and_web.md) |
| 7.5 | `flagd`: a feature-flag service with streaming updates | M | feature flags, SSE | T3, T9 | [07](07_networking_and_web.md) |
| 7.6 | `notifyd`: a notification system with retries and dedupe | M | notifications, queues, webhooks | T3, T9 | [07](07_networking_and_web.md) |
| 7.7 | `balancer`: a reverse proxy and load balancer | M | proxy, load balancing | T3, T9 | [07](07_networking_and_web.md) |
| 7.8 | `metricsx`: a metrics and observability library | L | metrics, observability | T3, T9 | [07](07_networking_and_web.md) |
| 8.1 | `benchlab`: a trustworthy micro-benchmark harness | S | benchmarking | T2, T5 | [08](08_performance_and_hpc.md) |
| 8.2 | `strfast`: small-string and interning micro-optimizations | S | small strings, interning | T2, T5 | [08](08_performance_and_hpc.md) |
| 8.3 | `gemm`: matrix multiply from naive to blocked and SIMD | M | matmul, SIMD, cache | T2, T5 | [08](08_performance_and_hpc.md) |
| 8.4 | `scanfast`: a SIMD byte scanner (memchr-style) | M | SIMD scanning | T2, T5 | [08](08_performance_and_hpc.md) |
| 8.5 | `colscan`: row vs column layouts and branchless filters | M | layouts, branchless | T2, T5 | [08](08_performance_and_hpc.md) |
| 8.6 | `allocs`: an allocator toolbox with a counting wrapper | M | allocators | T2, T5 | [08](08_performance_and_hpc.md) |
| 8.7 | `onebrc`: the One Billion Row Challenge, in Rust | L | 1BRC, profiling | T2, T5 | [08](08_performance_and_hpc.md) |
| 8.8 | `pipeq`: a lock-free pipeline at 10M messages/s | L | lock-free pipeline | T2, T5 | [08](08_performance_and_hpc.md) |
| 9.1 | `undo`: a command-pattern undo/redo stack for a text buffer | S | undo/redo | T4, T7 | [09](09_collaboration_and_editors.md) |
| 9.3 | `mdparse`: a CommonMark-subset parser to AST and HTML | M | markdown parser | T4, T7 | [09](09_collaboration_and_editors.md) |
| 9.4 | `blocks`: a block-editor document model with markdown import/export | M | block editor, markdown | T4, T7 | [09](09_collaboration_and_editors.md) |
| 9.5 | `tododoc`: a TODO app with CRDT sync and no database | M | CRDT, local-first TODO | T4, T7 | [09](09_collaboration_and_editors.md) |
| 9.6 | `rga`: a sequence CRDT for collaborative text | L | sequence CRDT | T4, T7 | [09](09_collaboration_and_editors.md) |
| 9.7 | `otsrv`: an operational-transform text server | L | OT server | T4, T7 | [09](09_collaboration_and_editors.md) |
| 9.8 | `syncd`: an offline-first sync engine | L | offline sync, HLC | T4, T7 | [09](09_collaboration_and_editors.md) |
| C1 | `rediska`: a Redis-compatible server with persistence and replication | XL | Redis, persistence, replication | 2+ themes | [10](10_capstone_portfolio.md) |
| C2 | `distsql`: a distributed SQL engine skeleton | XL | SQL, Raft, MVCC, exchange | 2+ themes | [10](10_capstone_portfolio.md) |
| C3 | `notebook`: a Notion-like collaborative editor with markdown and offline sync | XL | CRDT, markdown, search, sync | 2+ themes | [10](10_capstone_portfolio.md) |
| C4 | `vecdb`: a vector database with filters and an LSM store | XL | LSM, HNSW, filters, hybrid | 2+ themes | [10](10_capstone_portfolio.md) |
| C5 | `simlab`: a deterministic simulation test lab | XL | deterministic simulation, Raft, LSM | 2+ themes | [10](10_capstone_portfolio.md) |

## Files

- [01 Rust craft](01_rust_craft.md): CLI, newtypes, config diagnostics, typestate, plugin bus, std from scratch, publishable crate
- [02 Systems and concurrency](02_systems_and_concurrency.md): queues, sharded maps, thread pool, job scheduler, log pipeline, shell, async runtime, lock-free
- [03 Databases and storage](03_databases_and_storage.md): WAL, SSTable, LSM, B+tree, MVCC, SQL engine, columnar, optimizer
- [04 Distributed systems](04_distributed_systems.md): resilience client, hash ring, mini-Kafka, leases, Raft, Dynamo, SWIM
- [05 Compilers and languages](05_compilers_and_languages.md): Pratt, parsing, interpreter, VM, type inference, regex, LSP, SSA
- [06 Search, vectors and data](06_search_vectors_and_data.md): autocomplete, sketches, trigram, BM25, kNN/PQ, HNSW, MinHash, hybrid search
- [07 Networking and web](07_networking_and_web.md): HTTP server, URL shortener, REST service, rate limiter, flags, notifications, proxy, metrics
- [08 Performance and HPC](08_performance_and_hpc.md): bench harness, strings, matmul, SIMD, layouts, allocators, 1BRC, SPSC pipeline
- [09 Collaboration and editors](09_collaboration_and_editors.md): undo, rope, markdown, block editor, CRDT TODO, RGA, OT, offline sync
- [10 Capstone portfolio](10_capstone_portfolio.md): Redis-compatible server, distributed SQL, collaborative editor, vector DB, simulation lab

## Suggested sequences

- **Backend engineer:** 1.1 -> 7.1 -> 7.3 -> 7.4 -> 4.1 -> 7.2 -> 3.1 -> 3.3 -> C1.
- **Database nerd:** 3.1 -> 3.2 -> 3.3 -> 3.5 -> 3.6 -> 3.8 -> 4.5 -> C2.
- **Language/tooling:** 5.1 -> 5.2 -> 5.3 -> 5.4 -> 5.5 -> 5.7 -> (combined portfolio piece).
- **Performance:** 8.1 -> 8.2 -> 8.4 -> 8.3 -> 8.6 -> 8.7 -> 8.8 -> 2.8.
- **Local-first/collab:** 9.1 -> 9.3 -> 9.5 -> 9.2 -> 9.6 -> 9.8 -> C3.
- **Testing mindset:** 4.1 -> 4.4 -> 4.5 -> C5 (and add its simulator to every earlier project).
