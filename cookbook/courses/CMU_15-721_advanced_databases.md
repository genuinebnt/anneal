# CMU 15-721 Advanced Database Systems (topics from the Fall 2024 schedule)

Source: https://www.cs.cmu.edu/~15721-f24/schedule.html (Fall 2025 is taught by a different instructor; use its page for the latest reading list)

Coverage: 33 of 35 recipe slots exist.

| Lecture / topic | Recipes |
|---|---|
| Architecture of a Database System (Hellerstein, Stonebraker, Hamilton) | [`mini_kv_store`](../examples/capstones/mini_kv_store.rs) ✅<br>[`tiny_sql_engine`](../examples/capstones/tiny_sql_engine.rs) ✅<br>[`toy_filesystem`](../examples/capstones/toy_filesystem.rs) ✅ |
| Query optimization: Selinger / System R; extensible optimisers; join enumeration | [`query_optimizer`](../examples/capstones/query_optimizer.rs) ✅<br>[`cascades_memo_optimizer`](../examples/database/cascades_memo_optimizer.rs) ✅<br>[`join_enumeration_dpccp_dphyp`](../examples/database/join_enumeration_dpccp_dphyp.rs) ✅ |
| Adaptive query processing: Eddies, sideways information passing, robust plans | [`adaptive_query_processing_eddies_sip`](../examples/database/adaptive_query_processing_eddies_sip.rs) ✅ |
| High-performance query processing: MonetDB/X100, compiled vs vectorized | [`volcano_vectorized_push`](../examples/database/volcano_vectorized_push.rs) ✅<br>`vector_distance_kernels_simd` 🔜 |
| Joins: equi-join comparison, to-partition-or-not | [`join_algorithms`](../examples/database/join_algorithms.rs) ✅<br>[`radix_partitioned_hash_join`](../examples/database/radix_partitioned_hash_join.rs) ✅ |
| Parallel database systems (DeWitt/Gray) | [`parallel_query_morsel_exchange`](../examples/database/parallel_query_morsel_exchange.rs) ✅ |
| Transaction management: granularity of locks, OCC, Hekaton, MVCC | [`lock_manager_deadlock`](../examples/database/lock_manager_deadlock.rs) ✅<br>[`timestamp_ordering_occ`](../examples/database/timestamp_ordering_occ.rs) ✅<br>[`mvcc_version_storage_gc`](../examples/database/mvcc_version_storage_gc.rs) ✅ |
| Isolation levels: Adya, A Critique of ANSI SQL Isolation Levels | [`transaction_theory_serializability`](../examples/database/transaction_theory_serializability.rs) ✅<br>[`mvcc_snapshot_isolation`](../examples/database/mvcc_snapshot_isolation.rs) ✅ |
| Commit protocols: consensus on transaction commit, R* | [`two_phase_commit`](../examples/distributed/two_phase_commit.rs) ✅<br>[`paxos_basic_and_multi`](../examples/distributed/paxos_basic_and_multi.rs) ✅ |
| Advanced access methods: R-trees, GiST, bitmap/variant indexes, BitWeaving, ByteSlice | [`spatial_search_structures`](../examples/math_science/spatial_search_structures.rs) ✅<br>[`gist_generalized_search_tree`](../examples/data_structures/gist_generalized_search_tree.rs) ✅<br>[`hash_and_bitmap_indexes`](../examples/database/hash_and_bitmap_indexes.rs) ✅<br>[`bitweaving_bit_sliced_scans`](../examples/database/bitweaving_bit_sliced_scans.rs) ✅ |
| Dremel: nested columnar storage | [`dremel_nested_columnar_shredding`](../examples/database/dremel_nested_columnar_shredding.rs) ✅ |
| Snowflake: elastic data warehouse | [`cloud_warehouse_micro_partitions`](../examples/database/cloud_warehouse_micro_partitions.rs) ✅ |
| AI vector search in the database | [`ivf_flat`](../examples/search_vectors/ivf_flat.rs) ✅<br>[`hnsw_lite`](../examples/search_vectors/hnsw_lite.rs) ✅<br>[`product_quantization`](../examples/search_vectors/product_quantization.rs) ✅<br>`mini_vector_database` 🔜 |
| Spanner: TrueTime and SQL | [`spanner_truetime_commit_wait`](../examples/distributed/spanner_truetime_commit_wait.rs) ✅ |
| Lakehouse: Delta Lake, Spark RDDs | [`delta_lake_transaction_log`](../examples/database/delta_lake_transaction_log.rs) ✅ |
| Kafka: distributed log | [`kafka_log_model`](../examples/distributed/kafka_log_model.rs) ✅<br>[`record_batch_codec`](../examples/distributed/record_batch_codec.rs) ✅ |
| Redshift: columnar MPP warehouse | [`columnar_encodings`](../examples/database/columnar_encodings.rs) ✅<br>[`distributed_query_shuffle_broadcast_join`](../examples/database/distributed_query_shuffle_broadcast_join.rs) ✅ |

How to use: read the lecture's papers, attempt the recipe's asserts yourself (see ../HOW_TO_STUDY.md), then compare.
