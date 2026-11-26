# Practice: database

How to use this page: run the recipe first (`cargo run --release --example <name>`), then do Prove you got it before touching code.
Every Try next task names something you can check (an oracle, a counter, a fault injection); write that check first, then the change.
Do not look for solutions here; there are none. S is about 30 min, M about 2 h, A about a day. Recipe files live in `examples/database/`.

## adaptive_feedback_reoptimization
**Prove you got it**
1. Why does the LEO fingerprint have to be independent of join order, and what breaks if two plans of the same query hash differently?
2. For a checkpoint's validity range `[lo, hi]`, what happens at runtime when the observed cardinality is outside it, and what work is thrown away?

**Try next**
- [S] Change the miss fallback from "largest known sub-expression scaled by independence ratio" to plain independence; print the mean q-error after 1, 5, 20 repetitions of the workload for both.
- [M] Add noise: a data update between runs that changes one table 3x. Measure how many runs the repository needs to converge again, then add an expiry/versioning rule and show it converges faster.
- [A] Re-implement the repository alone as a library with `record(fingerprint, actual)` and `estimate(fingerprint)`, and drive it from the `cardinality_estimation_lab` generator; success = geometric-mean q-error improves on a held-out set of queries.

**Use it in:** a self-tuning optimizer that fixes plan regressions after the first slow run. Same idea: Spark AQE ([apache/spark](https://github.com/apache/spark)) re-plans at shuffle boundaries.
**Pitfall:** feedback learned on one predicate constant is wrongly applied to another; key the fingerprint on the constant or bucket it.

## adaptive_query_processing_eddies_sip
**Prove you got it**
1. In an eddy, what two per-operator numbers decide the next hop for a tuple, and why does the order change over time?
2. A Bloom runtime filter has a false-positive rate p; what is the worst a wrong positive can cost the join, and why can it never lose a result?

**Try next**
- [S] Vary the Bloom filter size over 4 values and plot or print work units against false-positive rate; find the size where the filter stops paying.
- [M] Add a drifting workload where filter pass rates swap halfway; assert the eddy's work stays within 1.3x of the best static order for each half.
- [A] Port the skew re-partitioning part onto real threads (std only), comparing wall-clock with a static hash partition on a Zipf key set.

**Use it in:** star-schema joins where a dimension filter prunes the fact scan. Same idea: runtime filters in [trinodb/trino](https://github.com/trinodb/trino).
**Pitfall:** building the filter costs a full pass over the build side; on a small or unselective build it is a net loss.

## aries_recovery
**Prove you got it**
1. After a crash, why must redo run on every record (even those of losers) before undo starts?
2. What does the page LSN do during redo, and what would double-apply if it were missing?

**Try next**
- [S] Crash in the middle of recovery itself (after N undo steps) for every N; assert the final state is identical to an uninterrupted recovery.
- [M] Add a torn page write (half the page persisted) and a per-record checksum; show recovery detects it, and say which part of ARIES cannot fix it alone (hint: doublewrite).
- [A] Reimplement the log + recovery as a standalone key-value store on real files with `fsync`, and test with a fault-injecting file wrapper that fails or truncates at a random write.

**Use it in:** any engine with steal/no-force buffering. Real code: [postgres/postgres](https://github.com/postgres/postgres) (`src/backend/access/transam/xlog.c`).
**Pitfall:** CLRs must have an undo-next pointer; without it, a crash during undo re-undoes work and corrupts data.

## bitweaving_bit_sliced_scans
**Prove you got it**
1. For a 9-bit code, how many word operations per 64 rows does a vertical `<` scan need, and why can it stop early?
2. Why does the horizontal layout need a delimiter bit per code?

**Try next**
- [S] Run all five layouts at widths 3, 9, 17, 31 and tabulate time per row; explain the crossover you measure.
- [M] Add BETWEEN as a single fused pass (not two scans ANDed) and assert equality with the naive result on 1000 random predicates.
- [A] Use `std::arch` or `std::simd` for the vertical layout and report the speedup, with a scalar fallback chosen at runtime.

**Use it in:** scanning dictionary-coded columns. Same idea: bit-unpacking kernels in [apache/arrow-rs](https://github.com/apache/arrow-rs) and [duckdb/duckdb](https://github.com/duckdb/duckdb).
**Pitfall:** the result bitmap's trailing bits past `n` must be masked or `count()` is wrong.

## buffer_pool_clock
**Prove you got it**
1. What makes it impossible to forget an unpin, and what does `fetch` return when everything is pinned?
2. Why does CLOCK approximate LRU but cost less per hit?

**Try next**
- [S] Replay one trace (80/20 hot set plus a sequential scan) through CLOCK, LRU, LRU-2 and print hit rate; explain the scan's effect on each.
- [M] Make the pool thread-safe (page table in a `Mutex`, per-frame `RwLock`); stress with 8 threads and assert no page is evicted while pinned.
- [A] Back the pool with a real file using `pread`/`pwrite`, add a background writer, and verify durability by killing and reopening.

**Use it in:** every page-oriented engine. Real code: [cmu-db/bustub](https://github.com/cmu-db/bustub) buffer pool assignment.
**Pitfall:** eviction writeback of a dirty page before its log record is flushed violates write-ahead logging.

## cardinality_estimation_lab
**Prove you got it**
1. Which estimator suffers most on Zipf-skewed foreign keys, and why does independence under-estimate there?
2. What does q-error measure and why is it symmetric?

**Try next**
- [S] Add a seventh table or a new join and report q-error for each estimator by number of joins.
- [M] Add a sampling-rate sweep for wander join; report error and cost for 5 rates and pick the knee.
- [A] Implement a correlated-column statistic (a 2D histogram or sampled joint MCV) and show it fixes one case independence gets wrong by 10x.

**Use it in:** judging an optimizer before trusting it. Same idea: the Join Order Benchmark approach; engine example [apache/datafusion](https://github.com/apache/datafusion).
**Pitfall:** errors compound multiplicatively along joins; averaging per-join q-error hides it.

## cascades_memo_optimizer
**Prove you got it**
1. Why are group expressions defined over groups instead of trees, and what does that save?
2. What does branch-and-bound prune, and what is the pruning bound?

**Try next**
- [S] Add a rule and show with a counter how many extra memo expressions it creates.
- [M] Add a physical property (sort order) with an enforcer; assert the plan with enforcer costs the same as an explicit sort on top.
- [A] Reproduce the optimal join order with the `join_enumeration_dpccp_dphyp` DP on 8 relations and assert both optimizers agree on cost.

**Use it in:** extensible SQL planners. Real code: [apache/calcite](https://github.com/apache/calcite) Volcano planner, [cockroachdb/cockroach](https://github.com/cockroachdb/cockroach) `pkg/sql/opt`.
**Pitfall:** applying a rule to its own output without a memo duplicate check explodes the search.

## cloud_warehouse_micro_partitions
**Prove you got it**
1. How does min/max metadata let a query skip a partition, and when is it useless?
2. Why is a zero-copy clone safe if files are immutable?

**Try next**
- [S] Run a query on a clustered and an unclustered table; print partitions scanned.
- [M] Add concurrent writers with a version check on the table pointer and test two clashing commits.
- [A] Persist partitions as real files and implement time travel to version N with garbage collection of unreferenced files, verified by a reachability oracle.

**Use it in:** warehouse and lakehouse tables. Same idea: [apache/iceberg](https://github.com/apache/iceberg).
**Pitfall:** GC deleting a file a long-running time-travel query still reads.

## columnar_encodings
**Prove you got it**
1. When does RLE beat dictionary encoding, and how does the encoder choose "cheapest"?
2. What does a zone map let a scan do without decoding?

**Try next**
- [S] Add a delta encoding for sorted timestamps and show the chosen-encoding histogram on a time series.
- [M] Add a null bitmap and prove (property test) every encoding round-trips `Option<i64>` over the full range including `i64::MIN`.
- [A] Write the column out in a Parquet-like file format with a footer, and read just one column using seeks only.

**Use it in:** column stores. Real code: [apache/arrow-rs](https://github.com/apache/arrow-rs) parquet crate.
**Pitfall:** zigzag/wrapping overflow at `i64::MIN`; test the extremes.

## delta_lake_transaction_log
**Prove you got it**
1. Why does put-if-absent on the next version number give you serialisable commits?
2. What must a writer re-check after losing the race?

**Try next**
- [S] Add checkpoint files every N commits and assert reading from checkpoint equals full replay.
- [M] Add a conflict matrix (append vs append OK, delete vs read same file not OK) and test 6 pairs.
- [A] Implement against a real directory with atomic rename, and run 4 processes committing concurrently; check no lost commit.

**Use it in:** lakehouse tables on S3. Real code: [delta-io/delta-rs](https://github.com/delta-io/delta-rs).
**Pitfall:** listing is not always strongly consistent on some stores; do not rely on it for the commit point.

## distributed_query_shuffle_broadcast_join
**Prove you got it**
1. When is broadcast cheaper than shuffle, expressed in bytes?
2. Why does two-phase aggregation reduce shipped bytes?

**Try next**
- [S] Sweep the small table size and find the crossover where the planner should switch.
- [M] Add a skewed key and implement a salted shuffle; assert max node load drops.
- [A] Run the nodes as threads with channels, count real bytes sent and match the model's prediction within 5%.

**Use it in:** MPP engines. Real code: [trinodb/trino](https://github.com/trinodb/trino), [apache/spark](https://github.com/apache/spark).
**Pitfall:** a model that ignores skew picks plans that are fast on average and time out on one node.

## dremel_nested_columnar_shredding
**Prove you got it**
1. What do repetition and definition levels each encode for a `NULL` inside a repeated field?
2. Why is the assembly an FSM?

**Try next**
- [S] Shred a record with an empty list and a list of nulls; explain why the level values differ.
- [M] Add a second repeated level and check round trip on 5000 random records.
- [A] Read only a subset of columns and assemble the partial records; compare with filtering after full assembly.

**Use it in:** Parquet. Real code: [apache/arrow-rs](https://github.com/apache/arrow-rs).
**Pitfall:** confusing "absent" with "null" at depth.

## external_sort_hash_aggregation
**Prove you got it**
1. How many passes does external sort need for N pages and B buffers?
2. What happens when one hash partition still does not fit?

**Try next**
- [S] Print IO for B=4,8,16 and compare to `2N(1+ceil(log_{B-1}(N/B)))`.
- [M] Add replacement selection and show runs of about 2B pages on random data.
- [A] Implement spilling on real temp files with checksums and an injected IO error; confirm cleanup.

**Use it in:** ORDER BY/GROUP BY. Real code: [duckdb/duckdb](https://github.com/duckdb/duckdb).
**Pitfall:** forgetting to recurse with a different hash seed on skew.

## hash_and_bitmap_indexes
**Prove you got it**
1. What happens to the directory when a bucket with l==g splits?
2. When does a bitmap index lose to a B-tree?

**Try next**
- [S] Insert 10k keys and print the final global depth and load factor.
- [M] Implement deletes with bucket merging; invariant: directory is shrinkable only if all local depths < g.
- [A] Build range-encoded bitmaps and answer 100 random range queries; check against a scan.

**Use it in:** warehouses, low cardinality columns. Real code: [postgres/postgres](https://github.com/postgres/postgres) hash index.
**Pitfall:** bitmaps on high-cardinality columns explode in size.

## join_algorithms
**Prove you got it**
1. Compute block nested-loop IO for given sizes by hand, then compare.
2. Why is sort-merge good when inputs are already sorted?

**Try next**
- [S] Add a count of output rows and check all algorithms agree on 200 seeded inputs.
- [M] Add duplicates-heavy keys (a few keys with 1000 rows) and watch which algorithm degrades.
- [A] Add full outer join and anti join to hash and merge variants, validated by the relational algebra oracle in `relational_algebra_engine`.

**Use it in:** every executor. Real code: [apache/datafusion](https://github.com/apache/datafusion).
**Pitfall:** NULL keys must not match in an equi-join.

## join_enumeration_dpccp_dphyp
**Prove you got it**
1. Why does DPccp enumerate fewer pairs than DPsize on a chain query?
2. When must you fall back to a heuristic?

**Try next**
- [S] Count pairs for chain, star, clique with 10 relations and tabulate.
- [M] Add a non-inner join edge and check DPhyp respects its ordering constraint.
- [A] Plug your enumerator into `cascades_memo_optimizer` as a join-order rule and compare plan cost.

**Use it in:** optimizers with many tables. Real code: [duckdb/duckdb](https://github.com/duckdb/duckdb) join order optimizer.
**Pitfall:** cost model bugs make a "best" plan wrong; check C_out by brute force on 5 relations.

## lock_manager_deadlock
**Prove you got it**
1. Why must you lock ancestors in intention mode first?
2. Name one deadlock detection and one prevention scheme here.

**Try next**
- [S] Write a 3-transaction cycle and assert the victim is the youngest.
- [M] Add timeout-based waiting and starvation detection with an assertion on bounded wait.
- [A] Run 16 threads on a real shared table with the lock manager and test serialisability via a precedence graph of the history.

**Use it in:** 2PL engines. Real code: [mongodb/mongo](https://github.com/mongodb/mongo) lock manager.
**Pitfall:** lock upgrades (S to X) between two txns is the classic deadlock.

## logging_schemes_group_commit
**Prove you got it**
1. Compare log bytes of the three schemes for the same workload.
2. How does group commit trade latency for throughput?

**Try next**
- [S] Vary the group window and report commits per fsync.
- [M] Add an async commit mode and a fault test: show which acknowledged txns can be lost.
- [A] Real fsync on a temp file; measure group vs single commit with 8 writer threads.

**Use it in:** any durable store. Real code: [postgres/postgres](https://github.com/postgres/postgres) WAL writer.
**Pitfall:** fsync of the file but not the directory after creating a log segment.

## lru_k_replacer_disk_scheduler
**Prove you got it**
1. Why does a sequential scan not wipe a hot set under LRU-K?
2. What does the disk scheduler return and who completes it?

**Try next**
- [S] Compare hit rate for K=1,2,3 with a scan-heavy trace.
- [M] Make requests carry priorities and show a read is not starved by a write flood.
- [A] Plug it into `buffer_pool_clock` as another policy with the same trait and compare on one trace.

**Use it in:** InnoDB-like pools. Real code: [cmu-db/bustub](https://github.com/cmu-db/bustub).
**Pitfall:** timestamps from a non-monotonic clock.

## lsm_compaction_strategies
**Prove you got it**
1. Rank leveled, tiered, lazy leveling by write/read/space amp.
2. What does Monkey do to bloom filter bits per level?

**Try next**
- [S] Print amplification for 3 write/read mixes and name the winner for each.
- [M] Add a delete-heavy workload with tombstones and show space amp over time.
- [A] Wire the chosen strategy into `mini_kv_store` (capstones) and compare read p99 before/after.

**Use it in:** RocksDB tuning. Real code: [facebook/rocksdb](https://github.com/facebook/rocksdb), [google/leveldb](https://github.com/google/leveldb).
**Pitfall:** compaction debt grows if flush outpaces compaction.

## mvcc_snapshot_isolation
**Prove you got it**
1. Construct a write skew history and show SI allows it.
2. What does first-committer-wins abort?

**Try next**
- [S] Add GC and assert no version needed by an active snapshot is removed.
- [M] Add SSI dangerous-structure detection and assert write skew now aborts.
- [A] Check random concurrent histories with the `transaction_theory_serializability` checker.

**Use it in:** PostgreSQL-style engines. Real code: [postgres/postgres](https://github.com/postgres/postgres).
**Pitfall:** long-running readers pin old versions forever.

## mvcc_version_storage_gc
**Prove you got it**
1. Why do index pointers differ between O2N and N2O?
2. Which GC style needs a background vacuum?

**Try next**
- [S] Count versions alive over time per design.
- [M] Add a long-running snapshot and measure GC lag per design.
- [A] Build a variant using delta storage with a chain-length cap, verified by the oracle in this file.

**Use it in:** choosing an MVCC storage layer. Real code: [postgres/postgres](https://github.com/postgres/postgres).
**Pitfall:** GC safe point must be the min active snapshot, not the latest commit.

## parallel_query_morsel_exchange
**Prove you got it**
1. What is a morsel and why is it sized in the thousands?
2. Why do parallel results equal serial exactly?

**Try next**
- [S] Sweep thread count 1..N and report scaling.
- [M] Add a skewed key and show work-stealing helps.
- [A] Add a parallel sort-merge join stage using sample sort.

**Use it in:** multicore analytics. Real code: [duckdb/duckdb](https://github.com/duckdb/duckdb).
**Pitfall:** false sharing on per-thread counters.

## parametric_query_optimization
**Prove you got it**
1. What is a plan diagram and what does anorexic reduction do?
2. Why can a generic plan lose to a custom plan?

**Try next**
- [S] Print the diagram for a coarser grid and count plans.
- [M] Add a robust-plan choice (min worst-case regret) and compare to the optimal pick.
- [A] Implement a plan cache with plan switching keyed by selectivity buckets, evaluated on a parameter stream.

**Use it in:** prepared statements. Real code: [postgres/postgres](https://github.com/postgres/postgres) plan cache.
**Pitfall:** parameter sniffing picks a plan for the first value only.

## plan_cache_multiplan_trial
**Prove you got it**
1. How does the trial decide a winner within a work budget?
2. When is a cached plan evicted?

**Try next**
- [S] Change the work budget and see winner stability.
- [M] Add a regression detector that evicts when later runs exceed 2x cached cost.
- [A] Add hint support (`INDEX`, `LEADING`) and prove hints cannot change results via an oracle.

**Use it in:** document DBs. Real code: [mongodb/mongo](https://github.com/mongodb/mongo) plan cache.
**Pitfall:** caching a trial winner chosen on unrepresentative data.

## query_rewrite_rules_magic_sets
**Prove you got it**
1. Why are rewrite rules run to a fixpoint by class?
2. What does magic sets restrict?

**Try next**
- [S] Add one rule (e.g. redundant DISTINCT) and a test showing it fires.
- [M] Add outer join simplification under a null-rejecting predicate; fuzz equivalence on random tables.
- [A] Apply magic sets to a recursive reachability query and compare tuples derived.

**Use it in:** logical planners. Real code: [apache/calcite](https://github.com/apache/calcite).
**Pitfall:** rules that are not semantics preserving under NULLs.

## radix_partitioned_hash_join
**Prove you got it**
1. Why does partitioning help once the hash table exceeds cache?
2. How many bits per pass, and why?

**Try next**
- [S] Sweep radix bits and print time.
- [M] Add multi-pass partitioning when fan-out exceeds TLB entries.
- [A] Compare against `join_algorithms` in-memory hash join on 10M rows with a measured speedup.

**Use it in:** analytics joins. Real code: [duckdb/duckdb](https://github.com/duckdb/duckdb).
**Pitfall:** partition skew makes one task dominate.

## relational_algebra_engine
**Prove you got it**
1. Give a bag-versus-set example where `Distinct` changes a result.
2. Why is pushdown below a join valid for a left-only predicate?

**Try next**
- [S] Add a rewrite and check equivalence on random relations.
- [M] Add outer joins and show pushdown is invalid on the null-supplying side.
- [A] Write a tiny SQL front end that lowers to this algebra (reuse `tiny_sql_engine` ideas).

**Use it in:** dataframes and planners. Real code: [apache/datafusion](https://github.com/apache/datafusion).
**Pitfall:** three-valued logic in selection.

## slotted_pages_heap_file
**Prove you got it**
1. Why does a Rid stay stable after compaction?
2. How does forwarding work when a record grows?

**Try next**
- [S] Fill pages randomly and print fragmentation.
- [M] Add overflow pages for big values and fuzz insert/update/delete against a `HashMap` oracle.
- [A] Persist to a file with a free-space map and verify after reopen.

**Use it in:** row stores. Real code: [sqlite/sqlite](https://github.com/sqlite/sqlite) btree pages.
**Pitfall:** off-by-one when slot array meets tuple data.

## sql_window_functions_ctes
**Prove you got it**
1. Difference between ROWS and RANGE frames with ties?
2. Why does `LAST_VALUE` surprise people by default?

**Try next**
- [S] Add `PERCENT_RANK` and verify against a hand-made case.
- [M] Add a recursive CTE cycle guard with a depth limit.
- [A] Implement a sliding aggregate with a segment tree and compare to naive on 1e5 rows.

**Use it in:** analytics SQL. Real code: [duckdb/duckdb](https://github.com/duckdb/duckdb).
**Pitfall:** NULL ordering defaults differ between engines.

## statistics_histograms_sketches
**Prove you got it**
1. When does equi-depth beat equi-width?
2. How does HLL merge across partitions?

**Try next**
- [S] Test estimates on a Zipf column and print error per type.
- [M] Add incremental maintenance on inserts and decide when to re-ANALYZE.
- [A] Persist stats and use them in the `capstones/query_optimizer`.

**Use it in:** ANALYZE. Real code: [postgres/postgres](https://github.com/postgres/postgres).
**Pitfall:** stats going stale after bulk loads.

## storage_models_nsm_dsm_pax
**Prove you got it**
1. For a 2-of-5 column scan, which layout reads fewest bytes?
2. Why is point lookup bad for DSM?

**Try next**
- [S] Add a wide update and compare the byte meter.
- [M] Add compression per column in DSM and report total bytes.
- [A] Make a hybrid chooser from a workload trace.

**Use it in:** OLTP vs OLAP. Real code: [apache/arrow-rs](https://github.com/apache/arrow-rs).
**Pitfall:** judging layouts by wall time on warm cache only.

## subquery_unnesting_decorrelation
**Prove you got it**
1. Why is `NOT IN` with NULLs special?
2. What is the COUNT bug?

**Try next**
- [S] Add EXISTS decorrelation test with an empty inner.
- [M] Add ALL/ANY with NULLs and test against a naive nested evaluator.
- [A] Fuzz random correlated queries comparing dependent-join and flattened plans.

**Use it in:** SQL planners. Real code: [duckdb/duckdb](https://github.com/duckdb/duckdb).
**Pitfall:** inner join instead of left join for scalar subqueries.

## timestamp_ordering_occ
**Prove you got it**
1. When does basic T/O abort a read?
2. What does OCC validate?

**Try next**
- [S] Compare abort rate for read-heavy and write-heavy mixes.
- [M] Add the Thomas write rule check on a history.
- [A] Run on threads and verify serialisability with `transaction_theory_serializability`.

**Use it in:** in-memory engines. Real code: [cockroachdb/cockroach](https://github.com/cockroachdb/cockroach) uses timestamp ordering variants.
**Pitfall:** livelock under high contention.

## transaction_theory_serializability
**Prove you got it**
1. Draw the precedence graph for a schedule with a cycle.
2. Which phenomenon is G2?

**Try next**
- [S] Add a recoverable-but-not-ACA example.
- [M] Add predicate reads and phantoms.
- [A] Use as the checker for histories from `mvcc_snapshot_isolation`.

**Use it in:** isolation testing. Real code: [jepsen-io/jepsen](https://github.com/jepsen-io/jepsen) Elle.
**Pitfall:** committed-only graphs hide aborted-read anomalies.

## volcano_vectorized_push
**Prove you got it**
1. Where does Volcano's per-tuple cost come from?
2. What is a selection vector?

**Try next**
- [S] Vary batch size 64..8192, report time.
- [M] Add a second aggregate and a join, keep results equal.
- [A] Use `perf` or flamegraph and explain the top 3 costs.

**Use it in:** engine design. Real code: [duckdb/duckdb](https://github.com/duckdb/duckdb), [apache/datafusion](https://github.com/apache/datafusion).
**Pitfall:** benchmarking without black_box.

## Chapter projects

Stems from other chapters used below: `bloom_filter` (search_vectors), `merkle_tree` (cryptography), `record_batch_codec` (distributed), `consistent_hashing` (distributed), `http_client_server_loopback` (web).

### Small (about 2 h): page-cached key index
Combine `slotted_pages_heap_file`, `buffer_pool_clock`, `bloom_filter`.
Milestones: (1) store records in slotted pages via a pool of 8 frames; (2) a Bloom filter in front for negative lookups; (3) report pool hit rate and filter savings.
Acceptance: a `HashMap` oracle agrees after 50k random ops; the filter never returns a false negative; no frame is evicted while pinned.
Stretch: swap CLOCK for `lru_k_replacer_disk_scheduler`'s replacer.

### Intermediate (a weekend): durable MVCC store
Combine `aries_recovery`, `mvcc_snapshot_isolation`, `logging_schemes_group_commit`, `merkle_tree`.
Milestones: (1) MVCC store with snapshots; (2) WAL with group commit; (3) crash anywhere and recover; (4) Merkle root of state after recovery compared with the pre-crash root.
Acceptance: fault injector kills at 200 random points, root always matches the last acknowledged commit; random histories pass the `transaction_theory_serializability` style checker.
Stretch: SSI.

### Advanced (a week): distributed query mini-warehouse
Combine `columnar_encodings`, `cloud_warehouse_micro_partitions`, `distributed_query_shuffle_broadcast_join`, `parallel_query_morsel_exchange`, `consistent_hashing`, `record_batch_codec`.
Milestones: (1) immutable partition files with zone maps; (2) shard by a consistent hash ring across threads; (3) shuffle and broadcast joins with a byte counter; (4) record-batch wire format between nodes; (5) join-order choice via `join_enumeration_dpccp_dphyp`.
Acceptance: results equal a single-node oracle on 10 TPC-H-like queries; adding a node moves about 1/N partitions; the planner's predicted bytes within 10% of measured.
Stretch: serve it over HTTP as in `http_client_server_loopback`.
