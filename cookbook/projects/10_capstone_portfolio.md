# 10. Capstone portfolio (XL, one month or more each)

Five projects that combine several themes. Each is large enough to be the one thing you talk about for 20 minutes in an interview, and each has milestones that ship independently, so you can stop at any milestone with something that works and is honest about what it is not.

How to run an XL project:
- **Week 0:** write `DESIGN.md` (goals, non-goals, architecture diagram in text, the 5 hardest design questions, test strategy). Revise it at every milestone; the diff of that file is your learning log.
- **Always keep an oracle and a deterministic simulator** (seeded RNG, injectable clock and network). Print the seed on failure.
- **Benchmarks are part of the deliverable** with a table in the README: machine, workload, p50/p99, throughput.
- **A README that lies is worse than none.** Add a "Known limitations" section.
- Do the smaller projects listed in "Prerequisite projects" first; each capstone reuses them.

Recipe stems are `folder/stem` under `examples/`. 🔜 = planned, not written yet.

---

## C1. `rediska`: a Redis-compatible server with persistence and replication  `XL`

**Goal.** A server that speaks RESP2 and works with `redis-cli` and `redis-benchmark`, supporting strings, lists, hashes, sets, sorted sets, TTL, pub/sub, transactions (`MULTI/EXEC`), persistence (AOF + snapshot) and primary/replica replication with partial resync.
**Prerequisite projects.** 2.1, 2.7 (or use tokio), 3.1, 3.2, 4.1, 7.1 for framing ideas.
**Read before.** `capstones/mini_redis`, `networking/resp_protocol`, `paths/building/cache_or_kv_server.md`. **During.** `data_structures/skiplist` (zset), `data_structures/lru_cache`, `os_and_storage/wal_append_log`, `os_and_storage/timing_wheel` (TTL), `async/worker_pool`, `distributed/kafka_log_model` (replication log ideas), `distributed/two_phase_commit` (what Redis does not do).
**Milestones.**
1. RESP2 parser/encoder with partial reads and pipelining; `PING`, `ECHO`, `GET/SET/DEL/EXISTS`, `INCR`. Works with `redis-cli`.
2. Data types: lists (`LPUSH/RPOP/LRANGE`), hashes, sets, sorted sets (`ZADD/ZRANGEBYSCORE/ZRANK` using a skiplist or order-statistic tree), 40 commands total with arity and type errors (`WRONGTYPE`) exactly like Redis.
3. Expiry: lazy on access + active sampling (20 keys per 100 ms, repeat if >25% expired); `PEXPIRE/TTL/PERSIST`.
4. Concurrency model decision: single-threaded command execution on one thread with I/O on others (Redis model) vs sharded per-core. Implement one, write why.
5. Persistence: AOF with `everysec`/`always` fsync policies and rewrite (compaction); RDB-like snapshot via copy-on-write structure or fork-free incremental iteration.
6. Replication: `REPLICAOF`; full sync (snapshot + buffered writes) then command stream; backlog ring buffer with offsets for partial resync (`PSYNC`).
7. Transactions (`MULTI/EXEC/DISCARD`, `WATCH`), pub/sub, `INFO` command with 20 fields.
8. Eviction: `maxmemory` with approximated LRU (sample 5) or LFU.
**Acceptance / invariants.**
- Differential test: run 100,000 random commands (seeded; a model with 6 types over 200 keys) against real Redis/Valkey and your server and compare replies byte-for-byte for deterministic commands.
- Crash test: `kill -9` at random points under `appendfsync always`: no acknowledged write is lost over 200 runs; under `everysec`, loss is bounded to about 1 s of writes.
- Replication: after a network cut of 5 s and reconnect, replica uses partial resync (log line + counter) and `DEBUG DIGEST`-style hash equals the primary's; replica kill and restart converges.
- Benchmark: `redis-benchmark -t set,get -n 1000000 -P 16 -c 50` reported against real Redis on the same box; target at least 50% of Redis throughput and p99 under 2 ms.
**Data and tooling.** `redis-cli`, `redis-benchmark`, Valkey/Redis binary for the oracle; the RESP spec; a command generator with typed arguments.
**Design write-up questions.** Why does single-threaded execution make transactions trivial? What does the AOF rewrite do while writes continue? What is the replication consistency guarantee (and why it is not linearizable)? What happens when the backlog is too small?
**Stretch.** Cluster mode with 16,384 hash slots, `MOVED/ASK` redirects and the 4.2 ring for slot assignment; streams (`XADD/XREADGROUP`) built on the 4.3 log; Lua-like scripting via your 5.4 VM.
**Real systems.** [Valkey](https://github.com/valkey-io/valkey), [Dragonfly](https://github.com/dragonflydb/dragonfly), [tokio-rs/mini-redis](https://github.com/tokio-rs/mini-redis), [RESP spec](https://redis.io/docs/latest/develop/reference/protocol-spec/).

## C2. `distsql`: a distributed SQL engine skeleton  `XL`

**Goal.** A cluster of nodes presenting one SQL interface: parse and plan queries, shard tables by key range or hash across nodes, run distributed joins and aggregations with exchange operators, use MVCC snapshot transactions, replicate each shard with Raft (or a simpler primary/backup first), and provide `EXPLAIN` showing the distributed plan.
**Prerequisite projects.** 3.3 or 3.4, 3.5, 3.6, 3.8, 4.2, 4.5.
**Read before.** `capstones/tiny_sql_engine`, `capstones/query_optimizer`, `database/distributed_query_shuffle_broadcast_join`, `distributed/raft_consensus`. **During.** `database/parallel_query_morsel_exchange`, `database/mvcc_snapshot_isolation`, `distributed/spanner_truetime_commit_wait`, `distributed/two_phase_commit`, `database/cascades_memo_optimizer`, `database/volcano_vectorized_push`, `distributed/hash_partitioning`.
**Milestones.**
1. Single node: SQL subset (`CREATE TABLE`, `INSERT`, `SELECT` with joins/aggregates, `UPDATE/DELETE` by primary key) on your own storage (3.3 LSM or 3.4 B+tree) with a catalog table.
2. Logical -> physical plan with rules (predicate pushdown, projection pruning, join order from 3.8) and `EXPLAIN`.
3. Sharding: hash or range partitions of tables across 3 nodes (processes); a router maps primary-key lookups to shards; scatter-gather for scans.
4. Distributed execution: exchange operators (shuffle by hash, broadcast, gather); hash join with broadcast of the small side vs shuffle chosen by size estimate; partial + final aggregation.
5. Transactions: MVCC with timestamps from a hybrid logical clock; single-shard fast path; cross-shard commit with two-phase commit over shard leaders; write conflict aborts.
6. Replication: each shard a Raft group of 3 (reuse 4.5); leader reads; leader failover under test.
7. Resilience: node loss during a query yields retry or a clear error, never a wrong result.
**Acceptance / invariants.**
- Differential: 500 generated queries over TPC-H-like tables (scale factor 0.01, 60k lineitems) match SQLite results (sorted).
- Bank invariant: 100 accounts across 3 shards, 8 clients, cross-shard transfers for 60 s with random node kills (leader-aware): total balance constant, no partial transfers (check by a read-only snapshot query running concurrently).
- Linearizability: single-key reads/writes history checked with the recipe `distributed/linearizability_checker` over 10,000 ops.
- Performance: distributed join of 1M x 1M rows on 3 nodes at least 2x faster than 1 node on the same data (report), or an honest explanation of the bottleneck.
**Data and tooling.** Seeded TPC-H-like generator (reuse 3.6), the simulator from 4.5 for deterministic fault injection, SQLite as oracle.
**Design write-up questions.** Where do you place the transaction coordinator and what if it dies after PREPARE? Why broadcast vs shuffle, and what is the break-even in rows? How do read timestamps interact with leader changes? What would you need for serializable (not just snapshot) isolation?
**Stretch.** Online shard split/merge; follower reads with bounded staleness; a columnar storage option via 3.7 for analytics tables.
**Real systems.** [CockroachDB](https://github.com/cockroachdb/cockroach), [TiDB](https://github.com/pingcap/tidb), [DataFusion](https://github.com/apache/datafusion), [Vitess](https://github.com/vitessio/vitess).

## C3. `notebook`: a Notion-like collaborative editor with markdown and offline sync  `XL`

**Goal.** A local-first block editor: pages made of typed blocks, rich text, real-time collaboration between clients, full offline editing with deterministic merge on reconnect, markdown import/export, full-text search across pages, and version history. A terminal UI (ratatui) or a small web UI is the front end; the engine is a Rust library with a sync server.
**Prerequisite projects.** 9.2, 9.3, 9.4, 9.5, 9.6 (or 9.7), 9.8, 6.4 (search).
**Read before.** `distributed/crdts`, `distributed/vector_clocks_hlc`, `distributed/operational_transform`, `patterns/undo_redo_editors`. **During.** `text_and_parsing/parser_combinators`, `search_vectors/trigram_index`, `os_and_storage/wal_append_log`, `networking/websocket_frames`, `web/axum_sse_replay`, `encoding_compression/compression_lz77_huffman`, `web/auth_basic_bearer_jwt`.
**Milestones.**
1. Document model: block tree CRDT (move operations resolved without cycles: use a move-op CRDT or LWW parent pointers with cycle check) + text CRDT inside each text block.
2. Local engine: op log persisted with WAL; snapshot every 1,000 ops; undo/redo applying only the local user's ops (selective undo).
3. Sync server: WebSocket, per-document op log, auth with bearer tokens, presence (cursors) as ephemeral state not in the log.
4. Offline: two clients edit for 10 simulated minutes offline, reconnect, converge; sync resumes after dropped connections without duplicates.
5. Markdown import/export: round trip for headings, lists, todos, code, links; unsupported blocks export as HTML comments preserving ids.
6. Search: incremental full-text index (BM25 or trigram) over block text, updated from the op stream; query latency under 20 ms over 100k blocks.
7. History: per-block version timeline and "restore to time T" built from the op log.
**Acceptance / invariants.**
- Convergence fuzz: 4 clients, 5,000 ops (insert/delete text, insert/move/delete/convert blocks, concurrent moves forming potential cycles) with random offline periods and shuffled delivery: all replicas equal (document hash) and tree acyclic, 1,000 seeds.
- Round trip: 200 README files import -> export -> import equal as ASTs.
- Offline resume: kill the client during sync 100 times at random points: zero lost ops, zero duplicates.
- Performance: a 1M-character document opens in under 300 ms from snapshot + log; typing latency (local apply) under 1 ms p99; document memory under 3x the plain text size for a typical editing history.
**Data and tooling.** The editing-trace datasets mentioned in 9.6, 200 READMEs, multi-client simulator with injectable faults; `ratatui` or a minimal HTML client.
**Design write-up questions.** CRDT or OT, and why? How do you move a block between pages concurrently with a delete of the destination? How are permissions enforced when the server cannot read content (E2EE option)? What is your tombstone GC policy and its failure mode?
**Stretch.** End-to-end encryption; a plugin API via WASM (`compilers_runtimes/wasm_binary_interpreter`); embedded images with content-addressed storage.
**Real systems.** [Yjs](https://github.com/yjs/yjs), [Automerge](https://github.com/automerge/automerge), [ProseMirror](https://github.com/ProseMirror/prosemirror), [Zed](https://github.com/zed-industries/zed) collaboration, [Local-first software](https://www.inkandswitch.com/local-first/).

## C4. `vecdb`: a vector database with filters and an LSM store  `XL`

**Goal.** A single-node vector database with collections, upsert/delete by id, payload (metadata) filtering, an HNSW index over immutable segments, an LSM-style write path (WAL -> memtable -> segment -> compaction), hybrid BM25 + vector queries, quantization, and a gRPC or REST API.
**Prerequisite projects.** 3.1, 3.2, 3.3, 6.4, 6.5, 6.6, 6.8, 7.3.
**Read before.** `search_vectors/hnsw_lite`, `search_vectors/product_quantization`, `database/lsm_compaction_strategies`, `paths/building/vector_search.md`. **During.** `search_vectors/vamana_diskann_lite`, `search_vectors/ivf_flat`, `data_structures/bitset_rank_select_roaring`, `os_and_storage/mmap_files`, `database/mvcc_version_storage_gc`, `perf/simd_intrinsics`, `capstones/search_engine`.
**Milestones.**
1. Storage: WAL + memtable of (id, vector f32, payload JSON); flush to immutable segment files (vectors contiguous, payload columnar, id map).
2. Per-segment index: brute-force for segments under 20,000 vectors, HNSW for larger; deletes via per-segment bitmap; upserts as delete + insert.
3. Query: search all segments + memtable, merge top-k; distance metrics L2/cosine/dot with SIMD.
4. Filters: payload indexes (keyword bitmap, numeric range); planner chooses pre-filter brute force vs filtered HNSW by selectivity (6.8).
5. Compaction: merge small segments (size-tiered), rebuild index, drop deleted vectors; queries run concurrently without blocking (readers hold segment `Arc`s).
6. Quantization: int8 scalar or PQ for in-memory index with rerank from full vectors on disk (mmap).
7. Hybrid: BM25 over a text payload field + RRF fusion; `explain` returns per-stage timings and candidates examined.
8. API + client + benchmark runner reproducing `ann-benchmarks`-style curves.
**Acceptance / invariants.**
- Oracle: exact brute-force search over all live vectors; recall@10 >= 0.95 at p99 under 20 ms on 1M x 128-d (SIFT1M) with filters at 100%, 10%, 1% selectivity.
- Crash consistency: kill -9 at random points during ingest and compaction (200 runs); after restart every acknowledged upsert is present and no deleted vector resurfaces.
- Concurrency: 8 writer threads + 8 reader threads for 60 s; every read sees a consistent snapshot (a segment set version), never a half-compacted state.
- Memory: 1M x 128 float vectors served with PQ under 200 MB RSS and recall@10 >= 0.9 after rerank.
**Data and tooling.** SIFT1M or GloVe-100 from ann-benchmarks; synthetic payloads (category, price, tags); the 6.5 harness for measurements.
**Design write-up questions.** Why immutable segments instead of an updatable graph? How do deletes affect HNSW connectivity and when do you rebuild? How does your planner estimate filter selectivity? Consistency of search after a write (read-your-writes)?
**Stretch.** Multi-vector documents (late interaction), sharding via 4.2 ring and scatter-gather, replication via 4.5.
**Real systems.** [Qdrant](https://github.com/qdrant/qdrant), [LanceDB](https://github.com/lancedb/lancedb), [RocksDB](https://github.com/facebook/rocksdb), [Tantivy](https://github.com/quickwit-oss/tantivy).

## C5. `simlab`: a deterministic simulation test lab  `XL`

**Goal.** A framework (in the FoundationDB / TigerBeetle / Madsim tradition) that runs distributed systems code on a single thread with a simulated clock, network and disk, all driven by one seed, so any failure is reproducible exactly, then use it to test your 4.5 Raft, 3.3 LSM and 4.3 mini-Kafka, finding (or seeding and then finding) real bugs.
**Prerequisite projects.** 2.7, 3.1, 4.1, 4.5; also useful: 3.3, 4.3.
**Read before.** `patterns/sans_io_protocol`, `distributed/linearizability_checker`, `distributed/raft_consensus`, `dev_tools/shrinking`, `async/waker_executor`. **During.** `capstones/lease_fencing_sim`, `distributed/chandy_lamport_snapshot`, `os_and_storage/wal_append_log`, `concurrency/memory_ordering_litmus`, `dev_tools/testing_patterns`, `capstones/mini_async_runtime`.
**Milestones.**
1. Core: simulated `Clock` (virtual time jumps to next event), `Rng` (single seeded PRNG), event queue with deterministic tie-breaking; `Runtime` trait abstracting spawn/sleep/now/random.
2. Simulated network: per-link latency distributions, drops, duplicates, reorders, partitions, asymmetric links, bandwidth limits; node crash and restart with lost volatile state.
3. Simulated disk: write buffering with fsync semantics, torn writes at sector boundaries, bit flips, `ENOSPC`, slow disk; reads can return stale-but-valid data after a crash.
4. Run unmodified protocol code (sans-I/O state machines or async code on your executor) on the simulator; ensure no real time/thread/hash-order nondeterminism leaks (lint: ban `Instant::now`, `HashMap` iteration order, `thread::spawn`).
5. Property library: linearizability checker (Wing-Gong/Knossos-style), invariants evaluated each step, liveness with timeouts ("eventually a leader exists after heal").
6. Shrinker: given a failing seed, minimize fault schedule (remove events, shorten partitions) while the failure persists.
7. Bug hunt: inject 5 known bugs into your Raft/LSM (e.g. commit with old-term entries, forget fsync before ack, off-by-one in log truncation, missing vote persistence, tombstone dropped early); the lab must catch each within 10,000 seeds and print a minimal reproduction.
8. Report: coverage of fault types, seeds per second (target 1,000+ short runs/s), a "swarm testing" mode randomizing which faults are enabled per run.
**Acceptance / invariants.**
- Determinism: same seed gives the same trace hash across 3 runs, across machines, and after a refactor that does not change semantics (CI check of the hash for 100 seeds).
- All 5 seeded bugs are found, each reproduced from `simlab replay --seed N` in under 1 s, with shrunken schedule at most 20 events.
- Fixed-code soak: 1,000,000 seeds (or 6 CPU-hours) with zero violations on your final Raft.
- Overhead: simulated 1 hour of a 5-node cluster completes in under 5 s wall time.
**Data and tooling.** The fault catalogue above, your three systems, `proptest`-style shrinking recipe; compare with `madsim` and `turmoil` behaviour for ideas.
**Design write-up questions.** What are the sources of nondeterminism in Rust programs and how did you remove each? What can simulation not find (hardware, kernel bugs, real timing)? How do you keep the simulated disk model honest versus real `fsync` semantics?
**Stretch.** Run the same protocol code on the real network under Jepsen-style fault injection (`tc netem`, `iptables` partitions) and compare which bugs each finds; record/replay of production traces.
**Real systems.** [FoundationDB](https://github.com/apple/foundationdb) (simulation framework), [TigerBeetle](https://github.com/tigerbeetle/tigerbeetle) (VOPR), [madsim](https://github.com/madsim-rs/madsim), [Jepsen](https://github.com/jepsen-io/jepsen), [loom](https://github.com/tokio-rs/loom).

---

## Capstone interview checklist

For each capstone you finish, be able to answer in two minutes each:
1. What is the single most important invariant, and which test would fail first if you broke it?
2. Show the seed/trace of a bug your test infrastructure found.
3. Which number in your benchmark table surprised you, and what did the profile say?
4. What did you deliberately not build, and why is that the right cut?
5. If the load grew 100x, what breaks first?

A language with an LSP and bytecode VM was considered as a sixth capstone; it is covered by projects 5.3, 5.4 and 5.7 (combine them into one repository with a shared parser crate if you want a portfolio piece).
