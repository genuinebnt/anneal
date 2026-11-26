# Track: Databases: a CMU 15-445 -> 15-721 -> 15-799 path

About 12-16 weeks. Course maps in ../../courses/ list every lecture; this is the build order.

**How to work each step:** read the header, predict the asserts, blank the file and rebuild it from memory (`./study.sh NAME`), then mutate it. Details in [HOW_TO_STUDY.md](../../HOW_TO_STUDY.md). Checkpoint = you can explain the step's goal in five lines without looking.

### Step 1: Storage and buffering

**Goal:** Pages, files, memory.

| # | Recipe | Why here |
|---|---|---|
| 1.1 | [`slotted_pages_heap_file`](../../examples/database/slotted_pages_heap_file.rs) · advanced | Pages and record ids. |
| 1.2 | [`storage_models_nsm_dsm_pax`](../../examples/database/storage_models_nsm_dsm_pax.rs) · advanced | Row vs column layouts. |
| 1.3 | [`buffer_pool_clock`](../../examples/database/buffer_pool_clock.rs) · advanced | Eviction basics. |
| 1.4 | [`lru_k_replacer_disk_scheduler`](../../examples/database/lru_k_replacer_disk_scheduler.rs) · advanced | LRU-K and a disk scheduler. |
| 1.5 | [`wal_append_log`](../../examples/os_and_storage/wal_append_log.rs) · advanced | Logging basics. |
| 1.6 | [`columnar_encodings`](../../examples/database/columnar_encodings.rs) · advanced | Dictionary, RLE, bit-packing. |

### Step 2: Indexes

**Goal:** Ordered, hash and modern structures.

| # | Recipe | Why here |
|---|---|---|
| 2.1 | [`bplus_tree`](../../examples/data_structures/bplus_tree.rs) · advanced | The baseline. |
| 2.2 | [`btree_bulk_load_prefix_compression`](../../examples/data_structures/btree_bulk_load_prefix_compression.rs) · advanced | Engineering a B+tree. |
| 2.3 | [`hash_and_bitmap_indexes`](../../examples/database/hash_and_bitmap_indexes.rs) · advanced | Hash and bitmap. |
| 2.4 | [`adaptive_radix_tree`](../../examples/data_structures/adaptive_radix_tree.rs) · advanced | ART. |
| 2.5 | [`optimistic_lock_coupling_btree`](../../examples/data_structures/optimistic_lock_coupling_btree.rs) · advanced | Concurrent B+trees. |
| 2.6 | [`bw_tree_latch_free`](../../examples/data_structures/bw_tree_latch_free.rs) · advanced | Latch-free index. |
| 2.7 | [`b_epsilon_tree`](../../examples/data_structures/b_epsilon_tree.rs) · advanced | Write-optimised. |
| 2.8 | [`lsm_compaction_strategies`](../../examples/database/lsm_compaction_strategies.rs) · advanced | LSM trade-offs. |
| 2.9 | [`learned_index_rmi`](../../examples/data_structures/learned_index_rmi.rs) · advanced | Learned indexes. |
| 2.10 | [`gist_generalized_search_tree`](../../examples/data_structures/gist_generalized_search_tree.rs) · advanced | Extensible indexing. |

### Step 3: Query execution

**Goal:** Operators and parallelism.

| # | Recipe | Why here |
|---|---|---|
| 3.1 | [`relational_algebra_engine`](../../examples/database/relational_algebra_engine.rs) · intermediate | Algebra and rewrites. |
| 3.2 | [`volcano_vectorized_push`](../../examples/database/volcano_vectorized_push.rs) · advanced | Execution models. |
| 3.3 | [`join_algorithms`](../../examples/database/join_algorithms.rs) · advanced | Join algorithms. |
| 3.4 | [`external_sort_hash_aggregation`](../../examples/database/external_sort_hash_aggregation.rs) · advanced | Sort and aggregate beyond memory. |
| 3.5 | [`radix_partitioned_hash_join`](../../examples/database/radix_partitioned_hash_join.rs) · advanced | Cache-conscious joins. |
| 3.6 | [`bitweaving_bit_sliced_scans`](../../examples/database/bitweaving_bit_sliced_scans.rs) · advanced | Scans on codes. |
| 3.7 | [`sql_window_functions_ctes`](../../examples/database/sql_window_functions_ctes.rs) · advanced | Windows and recursion. |
| 3.8 | [`parallel_query_morsel_exchange`](../../examples/database/parallel_query_morsel_exchange.rs) · advanced | Morsel-driven parallelism. |

### Step 4: Transactions and recovery

**Goal:** Correctness under concurrency and crashes.

| # | Recipe | Why here |
|---|---|---|
| 4.1 | [`transaction_theory_serializability`](../../examples/database/transaction_theory_serializability.rs) · advanced | Theory first. |
| 4.2 | [`lock_manager_deadlock`](../../examples/database/lock_manager_deadlock.rs) · advanced | 2PL. |
| 4.3 | [`timestamp_ordering_occ`](../../examples/database/timestamp_ordering_occ.rs) · advanced | T/O and OCC. |
| 4.4 | [`mvcc_snapshot_isolation`](../../examples/database/mvcc_snapshot_isolation.rs) · advanced | MVCC basics. |
| 4.5 | [`mvcc_version_storage_gc`](../../examples/database/mvcc_version_storage_gc.rs) · advanced | MVCC design space. |
| 4.6 | [`logging_schemes_group_commit`](../../examples/database/logging_schemes_group_commit.rs) · advanced | Logging schemes. |
| 4.7 | [`aries_recovery`](../../examples/database/aries_recovery.rs) · advanced | ARIES. |

### Step 5: Query optimisation

**Goal:** From rules to cost-based search.

| # | Recipe | Why here |
|---|---|---|
| 5.1 | [`statistics_histograms_sketches`](../../examples/database/statistics_histograms_sketches.rs) · advanced | Statistics. |
| 5.2 | [`cardinality_estimation_lab`](../../examples/database/cardinality_estimation_lab.rs) · advanced | Where estimates fail. |
| 5.3 | [`query_rewrite_rules_magic_sets`](../../examples/database/query_rewrite_rules_magic_sets.rs) · advanced | Rewrites. |
| 5.4 | [`subquery_unnesting_decorrelation`](../../examples/database/subquery_unnesting_decorrelation.rs) · advanced | Decorrelation. |
| 5.5 | [`join_enumeration_dpccp_dphyp`](../../examples/database/join_enumeration_dpccp_dphyp.rs) · advanced | Join ordering. |
| 5.6 | [`cascades_memo_optimizer`](../../examples/database/cascades_memo_optimizer.rs) · advanced | Cascades. |
| 5.7 | [`parametric_query_optimization`](../../examples/database/parametric_query_optimization.rs) · advanced | Parametric plans. |
| 5.8 | [`adaptive_feedback_reoptimization`](../../examples/database/adaptive_feedback_reoptimization.rs) · advanced | Feedback. |
| 5.9 | [`adaptive_query_processing_eddies_sip`](../../examples/database/adaptive_query_processing_eddies_sip.rs) · advanced | Runtime adaptivity. |
| 5.10 | [`plan_cache_multiplan_trial`](../../examples/database/plan_cache_multiplan_trial.rs) · advanced | Plan caching. |
| 5.11 | [`query_optimizer`](../../examples/capstones/query_optimizer.rs) · capstone | Capstone. |
| 5.12 | [`tiny_sql_engine`](../../examples/capstones/tiny_sql_engine.rs) · capstone | Capstone: end to end. |

### Step 6: Distributed and cloud databases

**Goal:** Beyond one machine.

| # | Recipe | Why here |
|---|---|---|
| 6.1 | [`distributed_query_shuffle_broadcast_join`](../../examples/database/distributed_query_shuffle_broadcast_join.rs) · advanced | Shuffle vs broadcast. |
| 6.2 | [`dremel_nested_columnar_shredding`](../../examples/database/dremel_nested_columnar_shredding.rs) · advanced | Nested columnar. |
| 6.3 | [`cloud_warehouse_micro_partitions`](../../examples/database/cloud_warehouse_micro_partitions.rs) · advanced | Warehouse design. |
| 6.4 | [`delta_lake_transaction_log`](../../examples/database/delta_lake_transaction_log.rs) · advanced | Lakehouse logs. |
| 6.5 | [`spanner_truetime_commit_wait`](../../examples/distributed/spanner_truetime_commit_wait.rs) · advanced | External consistency. |
