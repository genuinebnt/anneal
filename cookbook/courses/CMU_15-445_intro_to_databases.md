# CMU 15-445/645 Intro to Database Systems (Fall 2025 schedule)

Source: https://15445.courses.cs.cmu.edu/fall2025/schedule.html (lecture list read from the course site on 2026-11-25)

Coverage: 49 of 49 recipe slots exist.

| Lecture / topic | Recipes |
|---|---|
| 01 Relational Model & Algebra | [`relational_algebra_engine`](../examples/database/relational_algebra_engine.rs) ✅ |
| 02 Modern SQL (window functions, CTEs) | [`tiny_sql_engine`](../examples/capstones/tiny_sql_engine.rs) ✅<br>[`sql_window_functions_ctes`](../examples/database/sql_window_functions_ctes.rs) ✅ |
| 03 Database Storage I (pages, heap files) | [`slotted_pages_heap_file`](../examples/database/slotted_pages_heap_file.rs) ✅ |
| 04 Memory Management (buffer pool; replacement policies: LRU, CLOCK, LRU-K, 2Q, ARC, SIEVE) | [`buffer_pool_clock`](../examples/database/buffer_pool_clock.rs) ✅<br>[`cache_policy_zoo`](../examples/data_structures/cache_policy_zoo.rs) ✅<br>[`lru_k_replacer_disk_scheduler`](../examples/database/lru_k_replacer_disk_scheduler.rs) ✅ |
| 05 Database Storage II (log-structured, tuple layout) | [`mini_kv_store`](../examples/capstones/mini_kv_store.rs) ✅<br>[`lsm_compaction_strategies`](../examples/database/lsm_compaction_strategies.rs) ✅ |
| 06 Storage Models & Compression | [`columnar_encodings`](../examples/database/columnar_encodings.rs) ✅<br>[`storage_models_nsm_dsm_pax`](../examples/database/storage_models_nsm_dsm_pax.rs) ✅ |
| 07 Hash Tables | [`open_addressing_map`](../examples/data_structures/open_addressing_map.rs) ✅<br>[`cuckoo_hashing`](../examples/data_structures/cuckoo_hashing.rs) ✅<br>[`hash_and_bitmap_indexes`](../examples/database/hash_and_bitmap_indexes.rs) ✅<br>[`hash_functions`](../examples/data_structures/hash_functions.rs) ✅ |
| 08 Indexes & Filters I (B+tree, tries, skip lists) | [`bplus_tree`](../examples/data_structures/bplus_tree.rs) ✅<br>[`trie_radix`](../examples/data_structures/trie_radix.rs) ✅<br>[`skiplist`](../examples/data_structures/skiplist.rs) ✅<br>[`btree_bulk_load_prefix_compression`](../examples/data_structures/btree_bulk_load_prefix_compression.rs) ✅ |
| 09 Indexes & Filters II (filters, bitmaps, ART) | [`bloom_filter`](../examples/search_vectors/bloom_filter.rs) ✅<br>[`cuckoo_hashing`](../examples/data_structures/cuckoo_hashing.rs) ✅<br>[`bitset_rank_select_roaring`](../examples/data_structures/bitset_rank_select_roaring.rs) ✅<br>[`adaptive_radix_tree`](../examples/data_structures/adaptive_radix_tree.rs) ✅ |
| 10 Index Concurrency Control (latches, crabbing) | [`optimistic_lock_coupling_btree`](../examples/data_structures/optimistic_lock_coupling_btree.rs) ✅<br>[`bw_tree_latch_free`](../examples/data_structures/bw_tree_latch_free.rs) ✅ |
| 11 Sorting & Aggregation Algorithms | [`sorting_networks_external_sort`](../examples/algorithms/sorting_networks_external_sort.rs) ✅<br>[`external_sort_hash_aggregation`](../examples/database/external_sort_hash_aggregation.rs) ✅ |
| 12 Joins Algorithms | [`join_algorithms`](../examples/database/join_algorithms.rs) ✅ |
| 13 Query Execution I (processing models) | [`volcano_vectorized_push`](../examples/database/volcano_vectorized_push.rs) ✅ |
| 14 Query Execution II (parallel execution) | [`parallel_query_morsel_exchange`](../examples/database/parallel_query_morsel_exchange.rs) ✅ |
| 15-16 Query Planning & Optimization I-II | [`query_optimizer`](../examples/capstones/query_optimizer.rs) ✅<br>[`cascades_memo_optimizer`](../examples/database/cascades_memo_optimizer.rs) ✅ |
| 17 Concurrency Control Theory | [`transaction_theory_serializability`](../examples/database/transaction_theory_serializability.rs) ✅ |
| 18 Two-Phase Locking | [`lock_manager_deadlock`](../examples/database/lock_manager_deadlock.rs) ✅ |
| 19 Timestamp Ordering Concurrency Control | [`timestamp_ordering_occ`](../examples/database/timestamp_ordering_occ.rs) ✅ |
| 20 Multi-Version Concurrency Control | [`mvcc_snapshot_isolation`](../examples/database/mvcc_snapshot_isolation.rs) ✅<br>[`mvcc_version_storage_gc`](../examples/database/mvcc_version_storage_gc.rs) ✅ |
| 21 Database Logging | [`wal_append_log`](../examples/os_and_storage/wal_append_log.rs) ✅<br>[`logging_schemes_group_commit`](../examples/database/logging_schemes_group_commit.rs) ✅ |
| 22 Database Recovery | [`aries_recovery`](../examples/database/aries_recovery.rs) ✅ |
| 23-24 Distributed Database Systems I-II | [`two_phase_commit`](../examples/distributed/two_phase_commit.rs) ✅<br>[`raft_consensus`](../examples/distributed/raft_consensus.rs) ✅<br>[`paxos_basic_and_multi`](../examples/distributed/paxos_basic_and_multi.rs) ✅<br>[`quorum_replication_dynamo`](../examples/distributed/quorum_replication_dynamo.rs) ✅<br>[`distributed_query_shuffle_broadcast_join`](../examples/database/distributed_query_shuffle_broadcast_join.rs) ✅ |
| Projects: buffer pool, B+tree index, query executors, MVCC transactions | [`buffer_pool_clock`](../examples/database/buffer_pool_clock.rs) ✅<br>[`bplus_tree`](../examples/data_structures/bplus_tree.rs) ✅<br>[`tiny_sql_engine`](../examples/capstones/tiny_sql_engine.rs) ✅<br>[`mvcc_snapshot_isolation`](../examples/database/mvcc_snapshot_isolation.rs) ✅ |

How to use: read the lecture's papers, attempt the recipe's asserts yourself (see ../HOW_TO_STUDY.md), then compare.
