# 03. Databases and storage

From a single append-only file to an optimizer. Prerequisite track: `paths/tracks/06_databases_cmu.md` (and `11_os_storage_systems.md` for the I/O parts). Recipe stems are `folder/stem` under `examples/`. 🔜 = planned, not written yet.

Universal test strategy for this theme: a `BTreeMap` (or `Vec` of rows) oracle, seeded random op sequences of 10,000+, and **crash injection**: a wrapper `File` that truncates or drops writes at a random byte offset, then reopen and verify.

---

## 3.1 `walog`: a checksummed write-ahead log  `S`

**Goal.** An append-only log of records `[len u32][crc32 u32][payload]` with `append`, `sync`, `iter`, and recovery that stops at the first torn or corrupt record and truncates the tail.
**Learn.** fsync semantics, torn writes, CRC32, group commit, `File` positioning, `BufWriter` vs durability.
**Read before.** `os_and_storage/wal_append_log`, `database/logging_schemes_group_commit`. **During.** `os_and_storage/file_disk_io_patterns`, `os_and_storage/buffered_io_internals`, `encoding_compression/encodings_codecs`.
**Milestones.**
1. Append 10,000 records, reopen, iterate and compare.
2. Truncate the file at every byte offset of the last record (a loop): recovery returns exactly the complete prefix.
3. Flip one random bit per run in 1,000 runs: corruption is detected, never returned as data.
4. Group commit: N appender threads share one fsync; show fsync count << append count.
5. Segment rotation at 64 MiB with a manifest of segment ids.
**Acceptance.** For all truncation offsets in a 100-record log (about 20 KB) the recovered prefix length is monotone in offset; bit-flip detection rate is 100% over 10,000 trials (CRC32 miss rate is ~2^-32, so none expected).
**Data.** Payload sizes from a log-normal distribution, 0 bytes to 1 MiB, including empty payloads.
**Write-up.** What does your `sync` actually guarantee on macOS (`F_FULLFSYNC`) vs Linux? Which records may legitimately be lost after a crash?
**Stretch.** Pre-allocated, zero-filled segments and a sentinel for end-of-log (like RocksDB/Postgres).
**Resembles.** [RocksDB WAL](https://github.com/facebook/rocksdb), [etcd raft WAL](https://github.com/etcd-io/etcd).

## 3.2 `sstkit`: a sorted-string-table file with a Bloom filter  `S`

**Goal.** Write an immutable SSTable (data blocks of ~4 KiB with prefix compression, sparse index, Bloom filter, footer with offsets and magic number) and a reader supporting `get` and range `scan`.
**Learn.** On-disk format design, block index binary search, Bloom filter sizing (10 bits/key, 7 hashes ~1% FPR), `mmap` vs `pread`, versioned footers.
**Read before.** `search_vectors/bloom_filter`, `database/lsm_compaction_strategies`, `data_structures/btree_bulk_load_prefix_compression`. **During.** `os_and_storage/mmap_files`, `text_and_parsing/binary_format_parsing`, `perf/byte_order_keys`.
**Milestones.**
1. Writer takes sorted `(key, value)` iterator; rejects out-of-order input with an error.
2. Reader `get` using index + one block read; counts blocks touched.
3. Bloom filter: measure real FPR over 100,000 absent keys, within 0.5%-2% for 10 bits/key.
4. `scan(start..end)` returns the exact slice versus a `BTreeMap` oracle.
5. Footer magic + version; reader rejects a truncated or wrong-version file with a typed error.
**Acceptance.** 1M keys (16 B key, 100 B value), `get` on present key touches at most 1 data block; absent key touches 0 blocks in at least 98% of cases; file size within 10% of raw data size with prefix compression.
**Data.** Keys like `user:{:010}` (compress well) and random 16-byte keys (compress badly) to show the contrast.
**Write-up.** Why a sparse index and not one entry per key? What breaks if the writer is not deterministic?
**Stretch.** Block compression with your LZ77 recipe; a two-level index for >1 GB tables.
**Resembles.** [RocksDB BlockBasedTable](https://github.com/facebook/rocksdb), [SlateDB](https://github.com/slatedb/slatedb).

## 3.3 `lsmdb`: an LSM key-value store  `M`

**Goal.** Combine 3.1 and 3.2: memtable (skiplist/BTreeMap) + WAL, flush to level-0 SSTables, leveled or size-tiered compaction, tombstones, and a MANIFEST for atomic version changes.
**Learn.** Write path vs read path, write/read/space amplification, tombstone semantics, compaction scheduling, manifest-based atomic edits, iterators merging N sorted streams.
**Read before.** `database/lsm_compaction_strategies`, `data_structures/skiplist`, `capstones/mini_kv_store`. **During.** `database/mvcc_version_storage_gc`, `os_and_storage/file_locking_atomic_replace`, `algorithms/sorting_searching`.
**Milestones.**
1. `put/get/delete` in memory + WAL replay on restart.
2. Flush memtable at 4 MiB to an L0 SSTable; read path checks memtable, then L0 newest-first, then deeper levels.
3. Merging iterator (k-way heap) powering `scan`, hiding shadowed versions and tombstones.
4. Leveled compaction with fanout 10, L1 target 64 MiB; drop tombstones only at the bottom level.
5. MANIFEST (log of version edits) so a crash during compaction leaves either the old or the new file set.
**Acceptance.** 500,000 random ops (60% put, 20% delete, 20% get) vs `BTreeMap`: identical after every 1,000 ops and after 20 simulated crashes; write amplification measured and reported (bytes written to disk / bytes of user data); `get` of a present key in at most `L0 files + levels` SSTable probes.
**Data.** YCSB-style A (50/50) and C (read only) with Zipfian keys (reuse the 2.2 generator).
**Write-up.** When can a tombstone be dropped safely? What is your write stall policy when L0 has more than 8 files?
**Stretch.** Snapshots (sequence numbers), range deletes, block cache with LRU (`data_structures/lru_cache`).
**Resembles.** [RocksDB](https://github.com/facebook/rocksdb), [LevelDB](https://github.com/google/leveldb) is its ancestor (read `doc/impl.md` there).

## 3.4 `pagedb`: a disk B+tree with a buffer pool  `M`

**Goal.** A page-oriented (4 KiB) B+tree stored in a single file with a free list, a buffer pool with LRU-K or clock eviction and pin counts, supporting insert, get, delete and range scan.
**Learn.** Slotted pages, splits and merges, page ids vs pointers, pin/unpin discipline, dirty page write-back ordering, why the buffer pool is not the OS page cache.
**Read before.** `database/slotted_pages_heap_file`, `data_structures/bplus_tree`, `database/buffer_pool_clock`. **During.** `database/lru_k_replacer_disk_scheduler`, `data_structures/optimistic_lock_coupling_btree`, `os_and_storage/virtual_memory_sim`.
**Milestones.**
1. Page layout: header (type, count, free space) + slot array + cells; fixed key size first, variable later.
2. In-memory B+tree over page ids using the pool; capacity 64 frames.
3. Split on overflow with leaf links; root split grows height.
4. Delete with borrow/merge; maintain invariants (min fill 50% except root).
5. Persist: reopen file and read back; free list reclaims merged pages.
**Acceptance.** `check_invariants()` (sorted keys, separators correct, leaf chain complete, all leaves same depth, no pinned frames at rest) after every op in a 100,000-op random test vs `BTreeMap`; pool of 64 frames handles a 10,000-page tree (so evictions occur); hit-rate printed for sequential vs random access.
**Data.** Sequential inserts (worst case for splits), reverse order, and random; keys 8 B and variable 1..64 B.
**Write-up.** What is your policy if all frames are pinned? Which pages must be flushed before others for crash safety, and what do you not guarantee yet?
**Stretch.** Add the WAL from 3.1 for physical redo logging; bulk loading (`data_structures/btree_bulk_load_prefix_compression`).
**Resembles.** [SQLite B-tree](https://www.sqlite.org/arch.html), [TiKV](https://github.com/tikv/tikv) (RocksDB-based, contrast).

## 3.5 `mvccdb`: snapshot isolation key-value store  `M`

**Goal.** An in-memory multi-version store with `begin`, `get`, `put`, `commit`, `abort`, snapshot isolation, first-committer-wins write conflict detection and a background vacuum.
**Learn.** Version chains, visibility rules (xmin/xmax or begin/end timestamps), write skew, GC watermark, optimistic vs pessimistic concurrency.
**Read before.** `database/mvcc_snapshot_isolation`, `database/mvcc_version_storage_gc`, `database/transaction_theory_serializability`. **During.** `database/timestamp_ordering_occ`, `database/lock_manager_deadlock`, `distributed/linearizability_checker`.
**Milestones.**
1. Single-threaded: versions per key, reads see the snapshot as of `begin_ts`.
2. Concurrent transactions on threads; write-write conflict aborts the later committer.
3. Reproduce **write skew** (two doctors on call) and document that SI allows it.
4. Optional serializable mode: add read-set validation at commit (OCC) and show write skew is now rejected.
5. Vacuum: remove versions older than the minimum active snapshot; memory stays bounded in a 5-minute loop.
**Acceptance.** Bank-transfer invariant: 100 accounts, 8 threads, 100,000 transfers: total money constant, no negative balance under serializable mode; a history checker confirms no read of an uncommitted or aborted write; vacuum never removes a version a live snapshot can see (test by holding one long snapshot).
**Data.** The bank workload and the doctors-on-call script; a transaction trace generator with 3 keys and 3 txns for exhaustive interleaving.
**Write-up.** What exactly is your watermark and who advances it? What would you change to get serializable snapshot isolation (SSI)?
**Stretch.** Predicate reads (range scans) and phantom detection.
**Resembles.** [PostgreSQL](https://github.com/postgres/postgres) heap MVCC (contrast: version chains in the heap), [CockroachDB](https://github.com/cockroachdb/cockroach) and [TiKV](https://github.com/tikv/tikv) (percolator-style).

## 3.6 `sqlmini`: a SQL subset over CSV  `M`

**Goal.** `SELECT cols FROM t WHERE ... JOIN ... GROUP BY ... ORDER BY ... LIMIT` over CSV files: lexer, Pratt parser, logical plan, Volcano iterators, hash join and hash aggregate.
**Learn.** Operator trees, expression evaluation with SQL NULL (three-valued logic), schema inference, join algorithms, deterministic output ordering.
**Read before.** `capstones/tiny_sql_engine`, `database/relational_algebra_engine`, `database/join_algorithms`. **During.** `text_and_parsing/pratt_parser`, `database/volcano_vectorized_push`, `database/external_sort_hash_aggregation`, `text_and_parsing/csv_state_machine`.
**Milestones.**
1. Parse and execute `SELECT a, b FROM t WHERE a > 3`.
2. Expressions: arithmetic, `AND/OR/NOT`, `IS NULL`, `LIKE`, `IN (...)`; NULL semantics tested with a truth table.
3. `ORDER BY` with multi-key and `NULLS LAST`; `LIMIT/OFFSET`.
4. `GROUP BY` with `COUNT, SUM, AVG, MIN, MAX` (`AVG` of empty = NULL).
5. Inner and left hash join; then a nested-loop join as oracle.
**Acceptance.** Differential test against SQLite (`sqlite3` CLI loading the same CSV) on 200 generated queries over 3 tables x 1,000 rows: identical sorted results; 1M-row aggregate under 1 s release.
**Data.** TPC-H-like `customers/orders/lineitems` generator (seeded) with 5% NULLs and duplicate keys.
**Write-up.** Why is `NULL = NULL` unknown and how do you represent it? How would your executor change to be vectorized (batches of 1,024)?
**Stretch.** `EXPLAIN` printing the plan; predicate pushdown rule.
**Resembles.** [DataFusion](https://github.com/apache/datafusion), [DuckDB](https://github.com/duckdb/duckdb).

## 3.7 `colfile`: a columnar file format with vectorized scans  `L`

**Goal.** Store a table column-wise in row groups of 64k rows with per-column encoding (dictionary, RLE, delta + bit-packing), min/max zone maps, and a scan API that evaluates filters on encoded data in batches.
**Learn.** Column encodings, zone-map pruning, selection vectors, branch-free filtering, SoA layout, compression ratio vs decode speed.
**Read before.** `database/columnar_encodings`, `database/storage_models_nsm_dsm_pax`, `database/bitweaving_bit_sliced_scans`. **During.** `database/dremel_nested_columnar_shredding`, `search_vectors/knn_bruteforce` (SIMD style), `data_structures/bitset_rank_select_roaring`, `perf/branches_vectorization`.
**Milestones.**
1. Writer: row group with plain encoding + footer metadata; reader round-trips 1M rows, 5 columns.
2. Dictionary for low-cardinality strings, RLE for sorted ints, delta for timestamps; pick per column by trial.
3. Zone maps: a filter `ts BETWEEN a AND b` skips whole row groups (count skipped).
4. Vectorized filter producing a selection vector, then late materialization of other columns.
5. Aggregations `sum/min/max/count distinct (HLL)` on filtered data.
**Acceptance.** Compression at least 3x on the generated dataset vs CSV; filter+sum on 10M rows in under 100 ms release on a laptop (report the number); results equal a row-oriented oracle for 100 random queries.
**Data.** 10M rows: `ts` (monotone with jitter), `user_id` (Zipf, 100k distinct), `country` (30 values), `amount` (log-normal), `status` (4 values).
**Write-up.** Which encoding wins for which column and why? When does zone-map pruning not help?
**Stretch.** Parquet-compatible subset reader; Delta-style transaction log of file adds/removes (`database/delta_lake_transaction_log`).
**Resembles.** [Apache Arrow/Parquet in arrow-rs](https://github.com/apache/arrow-rs), [ClickHouse](https://github.com/ClickHouse/ClickHouse) and [DuckDB](https://github.com/duckdb/duckdb).

## 3.8 `optjoin`: a cost-based join-order optimizer  `L`

**Goal.** Given a query graph of N relations with predicates and table statistics, produce the cheapest join order using DP (DPsize or DPccp), with cardinality estimates from histograms and a cost model, then validate estimates against actual execution on your 3.6 engine.
**Learn.** Search space size, bushy vs left-deep, cardinality estimation error propagation, selectivity independence assumption, plan caching.
**Read before.** `database/join_enumeration_dpccp_dphyp`, `database/cardinality_estimation_lab`, `database/statistics_histograms_sketches`. **During.** `database/cascades_memo_optimizer`, `capstones/query_optimizer`, `database/plan_cache_multiplan_trial`, `database/adaptive_feedback_reoptimization`.
**Milestones.**
1. Query graph type + enumerate connected subgraph pairs; count them for chain/star/clique of n=10 (known counts: clique 3^n-ish; verify against brute force for n <= 8).
2. Equi-depth histograms and NDV (HLL) per column; selectivity for `=`, `<`, `AND`.
3. Cost model `C_out` (sum of intermediate cardinalities), then add hash-join build cost.
4. DP finds the optimum; compare with greedy and with random plans (median random plan should be much worse).
5. Run top-3 plans on real data and report q-error (max(est/actual, actual/est)) per join.
**Acceptance.** DP matches exhaustive search cost for all 200 random graphs with n <= 7; n=15 chain finishes in under 1 s; q-error median under 3 on uniform data, and you demonstrate (with correlated columns) a case where it exceeds 100.
**Data.** Generators for chain, star, cycle and clique query graphs; a correlated-column table (`city`, `zip`) to break independence.
**Write-up.** Why does estimation error compound with the number of joins? When would you stop DP and switch to a heuristic?
**Stretch.** Interesting orders (sort-merge) and a Cascades-style memo.
**Resembles.** [PostgreSQL planner](https://github.com/postgres/postgres) (`src/backend/optimizer`), [DataFusion](https://github.com/apache/datafusion), [CockroachDB](https://github.com/cockroachdb/cockroach) (Cascades-style).

---

## Katas (30 minutes each)

1. Write a CRC32 table generator and verify against the known check value `0xCBF43926` for `"123456789"`.
2. Encode integers with LEB128 varint; round-trip 100,000 random `u64` and assert max encoded length 10.
3. Compute the optimal Bloom filter `m` and `k` for n=1M, p=1%; verify empirically within 0.3% with 100,000 probes.
4. Write a slotted page that inserts 200 variable records into 4,096 bytes, deletes every 3rd, compacts, and reports free space before and after.
5. Implement a k-way merge of 10 sorted `Vec<u32>` with a `BinaryHeap<Reverse<_>>`; assert equals concatenate-then-sort for 1,000 random inputs.
6. Truncate your WAL at each offset in a 20-record file and print a table of offset -> records recovered.
7. Implement `LRU` with `HashMap` + doubly linked list via indices; compare hit ratio with CLOCK on a Zipf(1.0) trace of 1M accesses.
8. Show write skew with two threads and a `Vec<bool>` "on call" invariant; then prevent it with a lock on both rows.
9. Delta-encode a monotone timestamp column of 1M values and compute bits/value needed by max delta; print compression ratio.
10. Write a histogram (equi-depth, 32 buckets) and estimate `x < v` selectivity for 1,000 random `v`; report mean absolute error against the true fraction.
