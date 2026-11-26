# Chapter `database`: suggested reading order

Ordered simple -> intermediate -> advanced (then alphabetical). Skim `Use in:` and `COMMON USES` in each header first. Cross-chapter order is in [the tracks](../tracks/).

| # | Recipe | Level | What it is |
|---|---|---|---|
| 1 | [`relational_algebra_engine`](../../examples/database/relational_algebra_engine.rs) | intermediate | relational model and algebra: bags vs sets, operators, a typed expression evaluator, equivalence rewrites. |
| 2 | [`adaptive_feedback_reoptimization`](../../examples/database/adaptive_feedback_reoptimization.rs) | advanced | learning from execution: LEO-style feedback repository, mid-query re-optimization with checkpoints, plan stitching. |
| 3 | [`adaptive_query_processing_eddies_sip`](../../examples/database/adaptive_query_processing_eddies_sip.rs) | advanced | adaptive query processing at runtime: eddies, runtime filters (sideways information passing), join switching, skew re-partitioning, look-ahead information passing. |
| 4 | [`aries_recovery`](../../examples/database/aries_recovery.rs) | advanced | ARIES crash recovery: WAL with LSNs and CLRs, steal/no-force buffer pool, fuzzy checkpoints, analysis/redo/undo. |
| 5 | [`bitweaving_bit_sliced_scans`](../../examples/database/bitweaving_bit_sliced_scans.rs) | advanced | bit-parallel scans over compressed codes: BitWeaving/V, BitWeaving/H and ByteSlice. |
| 6 | [`buffer_pool_clock`](../../examples/database/buffer_pool_clock.rs) | advanced | a buffer pool: page frames, pin counts with an RAII guard, dirty write-back, CLOCK / LRU / LRU-2. |
| 7 | [`cardinality_estimation_lab`](../../examples/database/cardinality_estimation_lab.rs) | advanced | cardinality estimation lab: independence, histogram-join, wander-join sampling, Fast-AGMS sketches and a learned regressor on an IMDB-like schema. |
| 8 | [`cascades_memo_optimizer`](../../examples/database/cascades_memo_optimizer.rs) | advanced | a Cascades-style top-down optimiser: memo of groups, transformation + implementation rules, enforcers, branch-and-bound. |
| 9 | [`cloud_warehouse_micro_partitions`](../../examples/database/cloud_warehouse_micro_partitions.rs) | advanced | cloud data warehouse core ideas: micro-partitions, pruning metadata, clustering, time travel, zero-copy clone, elastic compute. |
| 10 | [`columnar_encodings`](../../examples/database/columnar_encodings.rs) | advanced | column-store encodings, zone maps and vectorised scans over encoded data. |
| 11 | [`delta_lake_transaction_log`](../../examples/database/delta_lake_transaction_log.rs) | advanced | Delta Lake style table format: a transaction log over an object store with optimistic concurrency. |
| 12 | [`distributed_query_shuffle_broadcast_join`](../../examples/database/distributed_query_shuffle_broadcast_join.rs) | advanced | distributed query processing: partitioning, pruning, two-phase aggregation, co-located/broadcast/shuffle/Bloom joins, skew, sample sort. |
| 13 | [`dremel_nested_columnar_shredding`](../../examples/database/dremel_nested_columnar_shredding.rs) | advanced | Dremel/Parquet nested record shredding: repetition and definition levels, the assembly FSM, level encoding. |
| 14 | [`external_sort_hash_aggregation`](../../examples/database/external_sort_hash_aggregation.rs) | advanced | external merge sort, top-N, and hash aggregation that spills (partition, recurse on skew), with IO counted in pages. |
| 15 | [`hash_and_bitmap_indexes`](../../examples/database/hash_and_bitmap_indexes.rs) | advanced | hash indexes (extendible, linear) and compressed bitmap indexes (equality/range encoded, WAH-style). |
| 16 | [`join_algorithms`](../../examples/database/join_algorithms.rs) | advanced | join algorithms compared: nested loop, block NL, index NL, hash, GRACE hash, sort-merge, outer/semi/anti, Bloom. |
| 17 | [`join_enumeration_dpccp_dphyp`](../../examples/database/join_enumeration_dpccp_dphyp.rs) | advanced | join enumeration: DPsize, DPsub, DPccp, DPhyp, top-down partitioning, and GOO/IDP/linearised heuristics for huge queries |
| 18 | [`lock_manager_deadlock`](../../examples/database/lock_manager_deadlock.rs) | advanced | a lock manager: multi-granularity intention locks, FIFO queues, upgrades, deadlock detection and prevention. |
| 19 | [`logging_schemes_group_commit`](../../examples/database/logging_schemes_group_commit.rs) | advanced | Write-ahead logging schemes (physical, logical, physiological), group commit, async commit, log ring buffer, checkpoints. |
| 20 | [`lru_k_replacer_disk_scheduler`](../../examples/database/lru_k_replacer_disk_scheduler.rs) | advanced | LRU-K replacer, a background disk scheduler with futures, and a concurrent buffer pool with page guards. |
| 21 | [`lsm_compaction_strategies`](../../examples/database/lsm_compaction_strategies.rs) | advanced | LSM compaction strategies (leveled, size-tiered, lazy leveling, FIFO) with exact amplification accounting and Monkey-style bloom filters. |
| 22 | [`mvcc_snapshot_isolation`](../../examples/database/mvcc_snapshot_isolation.rs) | advanced | MVCC snapshot isolation: version chains, first-committer-wins, GC, write skew, and SSI. |
| 23 | [`mvcc_version_storage_gc`](../../examples/database/mvcc_version_storage_gc.rs) | advanced | MVCC implementation choices: version storage (append-only O2N/N2O, time-travel, delta), garbage collection, index pointers. |
| 24 | [`parallel_query_morsel_exchange`](../../examples/database/parallel_query_morsel_exchange.rs) | advanced | parallel query execution: morsel-driven pipelines, exchange operators, parallel build/probe, sample sort. |
| 25 | [`parametric_query_optimization`](../../examples/database/parametric_query_optimization.rs) | advanced | parametric query optimization: plan diagrams, anorexic reduction, robust plans, bouquets, generic vs custom plans. |
| 26 | [`plan_cache_multiplan_trial`](../../examples/database/plan_cache_multiplan_trial.rs) | advanced | plan cache, plan baselines, MongoDB-style multi-plan trial, UDF optimisation and hints. |
| 27 | [`query_rewrite_rules_magic_sets`](../../examples/database/query_rewrite_rules_magic_sets.rs) | advanced | query rewrite rules over a Query Graph Model (view merging, pushdown, join/DISTINCT elimination, outer-join simplification, unnesting) and magic sets |
| 28 | [`radix_partitioned_hash_join`](../../examples/database/radix_partitioned_hash_join.rs) | advanced | in-memory hash joins: no-partition join vs radix-partitioned join vs sort-merge. |
| 29 | [`slotted_pages_heap_file`](../../examples/database/slotted_pages_heap_file.rs) | advanced | slotted pages and a heap file: variable-length tuples, stable record ids, compaction, forwarding, overflow pages. |
| 30 | [`sql_window_functions_ctes`](../../examples/database/sql_window_functions_ctes.rs) | advanced | modern SQL over in-memory tables: window functions, CTEs, recursive CTEs, LATERAL, GROUPING SETS. |
| 31 | [`statistics_histograms_sketches`](../../examples/database/statistics_histograms_sketches.rs) | advanced | optimizer statistics: histograms (equi-width, equi-depth, V-optimal, MaxDiff), MCV lists, NDV sketches, sampling, multi-column stats. |
| 32 | [`storage_models_nsm_dsm_pax`](../../examples/database/storage_models_nsm_dsm_pax.rs) | advanced | storage models: row store (NSM), column store (DSM) and PAX, with an IO and cache-line cost meter. |
| 33 | [`subquery_unnesting_decorrelation`](../../examples/database/subquery_unnesting_decorrelation.rs) | advanced | subquery unnesting and decorrelation with dependent joins (correlated scalar/EXISTS/IN/ANY/ALL/LATERAL -> joins). |
| 34 | [`timestamp_ordering_occ`](../../examples/database/timestamp_ordering_occ.rs) | advanced | Non-locking concurrency control: basic timestamp ordering, the Thomas write rule, optimistic CC (Kung-Robinson), and strict 2PL wait-die for comparison. |
| 35 | [`transaction_theory_serializability`](../../examples/database/transaction_theory_serializability.rs) | advanced | Transaction theory: schedules, conflict/view serializability, recoverability classes, ACID, and Adya's isolation phenomena. |
| 36 | [`volcano_vectorized_push`](../../examples/database/volcano_vectorized_push.rs) | advanced | one query, four execution models: Volcano, vectorized, push/data-centric, fused loop. |
