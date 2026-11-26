# Practice: capstones

How to use this page: run the capstone, read its header and main(), then answer Prove you got it before changing anything.
Capstones are big; the Try next tasks reuse their oracles and tests, so find the existing test first and extend it.
No solutions here. S about 1 h, M about half a day, A about 2-3 days. Files are in `examples/capstones/`. Do not attempt FlowForge stages from these; attempt your stage first.

## event_sourced_ledger
**Prove you got it**
1. What does `append(stream, expected_version, events)` return when another writer got there first, and what should the caller do?
2. Why must upcasters exist when events are immutable?

**Try next**
- [S] Add a transfer command and assert the sum of all balances is invariant over 1000 random commands.
- [M] Rebuild a projection after deleting it, and assert equality with the live one; add a snapshot every N events and assert faster rebuild with identical state.
- [A] Add an outbox relay with at-least-once delivery and an idempotent consumer; crash the relay at random points and assert no lost or double-applied event.

**Use it in:** banking, audit logs. Real code: [tigerbeetle/tigerbeetle](https://github.com/tigerbeetle/tigerbeetle) for ledger design.
**Pitfall:** changing an event's meaning in place instead of upcasting.

## job_service_axum
**Prove you got it**
1. How does the idempotency key interact with a cancelled job?
2. What does graceful shutdown wait for?

**Try next**
- [S] Add a job priority and assert higher priority starts first with a fixed worker count.
- [M] Add worker crash and lease expiry so a job is retried; assert each job completes once.
- [A] Persist the job table in a file with a WAL and restart mid-run; assert no job is lost or duplicated.

**Use it in:** background processing. Real code: [temporalio/temporal](https://github.com/temporalio/temporal).
**Pitfall:** marking done before the side effect is durable.

## lease_fencing_sim
**Prove you got it**
1. Why does a lease alone not protect a paused client?
2. What does the storage check with fencing tokens?

**Try next**
- [S] Run 1000 seeds and assert enforced fencing never accepts a stale write.
- [M] Add clock skew between the lease service and clients and find the skew at which the lease-only variant fails first.
- [A] Compare with the Raft-backed lease in `raft_consensus` (distributed) and measure latency and safety under partitions.

**Use it in:** distributed locks. Real code: [etcd-io/raft](https://github.com/etcd-io/raft) backs etcd leases.
**Pitfall:** the token must be checked at the resource, not just by the client.

## mini_async_runtime
**Prove you got it**
1. What does a waker do and who calls it?
2. Why does work stealing help and what does it cost?

**Try next**
- [S] Add a `yield_now` and assert fairness between two busy tasks.
- [M] Add task cancellation with drop of the future and assert destructors run.
- [A] Port an example from `mini_http_server_thread_pool` onto this runtime and compare throughput.

**Use it in:** understanding tokio. Real code: [tokio-rs/tokio](https://github.com/tokio-rs/tokio).
**Pitfall:** lost wakeups between poll and register.

## mini_http_server_thread_pool
**Prove you got it**
1. What does a bounded queue do when full?
2. How is pipelining handled?

**Try next**
- [S] Add `HEAD` and a conditional GET test.
- [M] Add a request timeout and a slowloris test with a half-sent header.
- [A] Run a load test at 1, 8, 64 connections and report requests per second and p99, then explain the curve.

**Use it in:** embedded servers. Real code: [hyperium/hyper](https://github.com/hyperium/hyper).
**Pitfall:** path traversal in the static file handler.

## mini_kv_store
**Prove you got it**
1. What does a tombstone protect against?
2. How does recovery decide where the WAL ends?

**Try next**
- [S] Assert the store equals a `BTreeMap` oracle after 100k random operations including restarts.
- [M] Truncate the WAL at every byte offset and assert recovery returns a prefix of acknowledged writes.
- [A] Add size-tiered or leveled compaction from `lsm_compaction_strategies` (database) and compare write amplification.

**Use it in:** embedded storage. Real code: [google/leveldb](https://github.com/google/leveldb), [facebook/rocksdb](https://github.com/facebook/rocksdb), [cberner/redb](https://github.com/cberner/redb).
**Pitfall:** forgetting fsync of the directory after creating a file.

## mini_redis
**Prove you got it**
1. Why does one executor thread own all data?
2. How do expiry and AOF interact?

**Try next**
- [S] Add `INCRBY` and `SETNX` with tests via the protocol.
- [M] Add `MULTI/EXEC` with a conflict test, and AOF rewrite that preserves state.
- [A] Run a differential test of a command stream against a real Redis if installed ([redis/redis](https://github.com/redis/redis)).

**Use it in:** caches. Real code: [tokio-rs/mini-redis](https://github.com/tokio-rs/mini-redis), [redis/redis](https://github.com/redis/redis).
**Pitfall:** blocking the executor thread with disk IO.

## query_optimizer
**Prove you got it**
1. Where does the estimated plan most often go wrong and how does the q-error show it?
2. Why do rewrite rules come before join ordering?

**Try next**
- [S] Print estimated vs true rows per operator for 10 queries.
- [M] Add an index scan alternative and assert the planner chooses it when selectivity is below a threshold you derive.
- [A] Swap the join ordering for DPccp from `join_enumeration_dpccp_dphyp` (database) and compare plan cost on 8 tables.

**Use it in:** SQL planners. Real code: [apache/datafusion](https://github.com/apache/datafusion), [postgres/postgres](https://github.com/postgres/postgres).
**Pitfall:** trusting estimates beyond 3 joins.

## rate_limiter_lab
**Prove you got it**
1. Which algorithm allows a 2x burst at a window boundary?
2. What does AIMD do in the concurrency limiter?

**Try next**
- [S] Add a test of burst at the boundary for fixed window and show others differ.
- [M] Run 4 keys with unequal demand and compute a fairness index per algorithm.
- [A] Add a distributed variant with local plus global budgets and measure over-admission versus the number of nodes.

**Use it in:** gateways. Real code: [boinkor-net/governor](https://github.com/boinkor-net/governor), [envoyproxy/envoy](https://github.com/envoyproxy/envoy) rate limit.
**Pitfall:** per-key maps that never evict.

## ray_tracer
**Prove you got it**
1. What does the BVH change in complexity?
2. Why do fireflies appear in path tracing?

**Try next**
- [S] Add a new primitive (plane or cylinder) and a test image hash.
- [M] Render with 1, 4, 16 samples per pixel and report noise (RMS vs the 256 spp image).
- [A] Parallelise tiles over threads with a deterministic seed per pixel and check the image is byte-identical to the serial render.

**Use it in:** graphics learning. Real code: the "Ray Tracing in One Weekend" book series is the usual reference.
**Pitfall:** shadow acne from missing epsilon.

## regex_nfa_engine
**Prove you got it**
1. Why is `(a*)*b` exponential for a backtracker and linear here?
2. How do anchors work in the simulation?

**Try next**
- [S] Add `{m,n}` repetition and test against the backtracker.
- [M] Add captures (Pike VM) and check against a reference on 1000 random patterns.
- [A] Compile to a lazy DFA and compare throughput with the NFA, using [rust-lang/regex](https://github.com/rust-lang/regex) as a speed reference.

**Use it in:** search tools. Real code: [rust-lang/regex](https://github.com/rust-lang/regex) (`regex-automata`).
**Pitfall:** empty-loop handling in star of nullable expressions.

## search_engine
**Prove you got it**
1. Why are segments immutable and how are deletes handled?
2. How does BM25 treat term frequency saturation?

**Try next**
- [S] Add a phrase-query test against a naive scan.
- [M] Add segment merge and assert query results are unchanged.
- [A] Compare ranking quality against `trigram_index`-only results on a labelled mini set.

**Use it in:** site search. Real code: [quickwit-oss/tantivy](https://github.com/quickwit-oss/tantivy), [apache/lucene](https://github.com/apache/lucene).
**Pitfall:** averaging document lengths per segment instead of globally.

## tiny_sql_engine
**Prove you got it**
1. Where is NULL handled?
2. What does the binder resolve that the parser cannot?

**Try next**
- [S] Add `DELETE` and `UPDATE` with tests.
- [M] Add `JOIN` through a hash join and check against a nested-loop version.
- [A] Differential-test 500 random queries against SQLite ([sqlite/sqlite](https://github.com/sqlite/sqlite)) if installed.

**Use it in:** query engines. Real code: [apache/datafusion-sqlparser-rs](https://github.com/apache/datafusion-sqlparser-rs), [apache/datafusion](https://github.com/apache/datafusion).
**Pitfall:** `GROUP BY` of NULLs.

## toy_filesystem
**Prove you got it**
1. What does the journal guarantee after a crash?
2. How does fsck detect a leaked block?

**Try next**
- [S] Add a link count test.
- [M] Crash after every block write in a sequence and assert fsck + replay yields a consistent tree.
- [A] Add a read-only FUSE-like layer or a CLI that lists a populated image.

**Use it in:** storage engines. Real code: ext4/xv6 sources; for Rust see [cberner/redb](https://github.com/cberner/redb).
**Pitfall:** ordering of data and metadata writes.

## toy_language_interpreter
**Prove you got it**
1. What does the resolver do before execution?
2. Why test the tree walker against the VM?

**Try next**
- [S] Add a built-in and a test.
- [M] Add a `for in` over lists and test with break/continue.
- [A] Add a tracing GC or reference-counted cycle collector and a stress test.

**Use it in:** scripting. Real code: [gleam-lang/gleam](https://github.com/gleam-lang/gleam), `crates/cb-compiler` here.
**Pitfall:** closures capturing loop variables.

## typeahead_service
**Prove you got it**
1. How does the cache know when it is stale?
2. What merges trie and trigram results?

**Try next**
- [S] Add a test that cache returns after an index update.
- [M] Add per-user personalisation with a bound on memory.
- [A] Load test and report p50/p99 with 10k terms and with 1M terms.

**Use it in:** search boxes. Real code: [meilisearch/meilisearch](https://github.com/meilisearch/meilisearch).
**Pitfall:** cache poisoning from unnormalised queries.

## Chapter projects

Other-chapter stems used: `consistent_hashing`, `raft_consensus`, `retry_backoff_jitter` (distributed), `aries_recovery`, `lsm_compaction_strategies` (database), `chacha20_poly1305` (cryptography), `bloom_filter` (search_vectors), `pratt_parser` (text_and_parsing).

### Small (about 2 h): extend a capstone with a test oracle
Combine `mini_kv_store`, `bloom_filter`.
Milestones: (1) a seeded operation generator; (2) `BTreeMap` oracle; (3) restart at random points.
Acceptance: 10 seeds pass; deleting the WAL's last record gives a valid earlier state.
Stretch: report bloom filter skip rate.

### Intermediate (a weekend): replicated redis-like cache
Combine `mini_redis`, `consistent_hashing`, `retry_backoff_jitter`, `rate_limiter_lab`.
Milestones: (1) client library routing by hash; (2) retries with backoff; (3) per-key limits; (4) a node join with key migration.
Acceptance: migration loses no keys; the limiter's admitted rate stays within 5% of target.
Stretch: replicate with `raft_consensus` ideas.

### Advanced (a week): mini database
Combine `tiny_sql_engine`, `mini_kv_store`, `query_optimizer`, `aries_recovery`, `pratt_parser`.
Milestones: (1) SQL front end; (2) KV-backed tables; (3) WAL with recovery; (4) cost-based plans; (5) differential tests against SQLite.
Acceptance: 1000 random queries equal SQLite's results; crash at 100 points loses no acknowledged transaction; EXPLAIN shows chosen plan.
Stretch: MVCC snapshots.
