#!/usr/bin/env python3
"""Generate courses/*.md: lecture-by-lecture maps from CMU courses to cookbook recipes.
A topic is marked done when examples/<chapter>/<name>.rs exists, otherwise planned. Run: python3 courses.py (index.py calls it)."""
import glob, os
have = {os.path.basename(f)[:-3]: f.split("/")[1] for f in glob.glob("examples/*/*.rs") if "/scratch/" not in f}
def mark(names):
    out = []
    for n in names:
        out.append(f"[`{n}`](../examples/{have[n]}/{n}.rs) ✅" if n in have else f"`{n}` 🔜")
    return "<br>".join(out) if out else "(theory/reading only)"

COURSES = {
"CMU_15-445_intro_to_databases": {
 "title": "CMU 15-445/645 Intro to Database Systems (Fall 2025 schedule)",
 "source": "https://15445.courses.cs.cmu.edu/fall2025/schedule.html (lecture list read from the course site on 2026-11-25)",
 "rows": [
  ("01 Relational Model & Algebra", ["relational_algebra_engine"]),
  ("02 Modern SQL (window functions, CTEs)", ["tiny_sql_engine", "sql_window_functions_ctes"]),
  ("03 Database Storage I (pages, heap files)", ["slotted_pages_heap_file"]),
  ("04 Memory Management (buffer pool; replacement policies: LRU, CLOCK, LRU-K, 2Q, ARC, SIEVE)", ["buffer_pool_clock", "cache_policy_zoo", "lru_k_replacer_disk_scheduler"]),
  ("05 Database Storage II (log-structured, tuple layout)", ["mini_kv_store", "lsm_compaction_strategies"]),
  ("06 Storage Models & Compression", ["columnar_encodings", "storage_models_nsm_dsm_pax"]),
  ("07 Hash Tables", ["open_addressing_map", "cuckoo_hashing", "hash_and_bitmap_indexes", "hash_functions"]),
  ("08 Indexes & Filters I (B+tree, tries, skip lists)", ["bplus_tree", "trie_radix", "skiplist", "btree_bulk_load_prefix_compression"]),
  ("09 Indexes & Filters II (filters, bitmaps, ART)", ["bloom_filter", "cuckoo_hashing", "bitset_rank_select_roaring", "adaptive_radix_tree"]),
  ("10 Index Concurrency Control (latches, crabbing)", ["optimistic_lock_coupling_btree", "bw_tree_latch_free"]),
  ("11 Sorting & Aggregation Algorithms", ["sorting_networks_external_sort", "external_sort_hash_aggregation"]),
  ("12 Joins Algorithms", ["join_algorithms"]),
  ("13 Query Execution I (processing models)", ["volcano_vectorized_push"]),
  ("14 Query Execution II (parallel execution)", ["parallel_query_morsel_exchange"]),
  ("15-16 Query Planning & Optimization I-II", ["query_optimizer", "cascades_memo_optimizer"]),
  ("17 Concurrency Control Theory", ["transaction_theory_serializability"]),
  ("18 Two-Phase Locking", ["lock_manager_deadlock"]),
  ("19 Timestamp Ordering Concurrency Control", ["timestamp_ordering_occ"]),
  ("20 Multi-Version Concurrency Control", ["mvcc_snapshot_isolation", "mvcc_version_storage_gc"]),
  ("21 Database Logging", ["wal_append_log", "logging_schemes_group_commit"]),
  ("22 Database Recovery", ["aries_recovery"]),
  ("23-24 Distributed Database Systems I-II", ["two_phase_commit", "raft_consensus", "paxos_basic_and_multi", "quorum_replication_dynamo", "distributed_query_shuffle_broadcast_join"]),
  ("Projects: buffer pool, B+tree index, query executors, MVCC transactions", ["buffer_pool_clock", "bplus_tree", "tiny_sql_engine", "mvcc_snapshot_isolation"]),
 ]},
"CMU_15-721_advanced_databases": {
 "title": "CMU 15-721 Advanced Database Systems (topics from the Fall 2024 schedule)",
 "source": "https://www.cs.cmu.edu/~15721-f24/schedule.html (Fall 2025 is taught by a different instructor; use its page for the latest reading list)",
 "rows": [
  ("Architecture of a Database System (Hellerstein, Stonebraker, Hamilton)", ["mini_kv_store", "tiny_sql_engine", "toy_filesystem"]),
  ("Query optimization: Selinger / System R; extensible optimisers; join enumeration", ["query_optimizer", "cascades_memo_optimizer", "join_enumeration_dpccp_dphyp"]),
  ("Adaptive query processing: Eddies, sideways information passing, robust plans", ["adaptive_query_processing_eddies_sip"]),
  ("High-performance query processing: MonetDB/X100, compiled vs vectorized", ["volcano_vectorized_push", "vector_distance_kernels_simd"]),
  ("Joins: equi-join comparison, to-partition-or-not", ["join_algorithms", "radix_partitioned_hash_join"]),
  ("Parallel database systems (DeWitt/Gray)", ["parallel_query_morsel_exchange"]),
  ("Transaction management: granularity of locks, OCC, Hekaton, MVCC", ["lock_manager_deadlock", "timestamp_ordering_occ", "mvcc_version_storage_gc"]),
  ("Isolation levels: Adya, A Critique of ANSI SQL Isolation Levels", ["transaction_theory_serializability", "mvcc_snapshot_isolation"]),
  ("Commit protocols: consensus on transaction commit, R*", ["two_phase_commit", "paxos_basic_and_multi"]),
  ("Advanced access methods: R-trees, GiST, bitmap/variant indexes, BitWeaving, ByteSlice", ["spatial_search_structures", "gist_generalized_search_tree", "hash_and_bitmap_indexes", "bitweaving_bit_sliced_scans"]),
  ("Dremel: nested columnar storage", ["dremel_nested_columnar_shredding"]),
  ("Snowflake: elastic data warehouse", ["cloud_warehouse_micro_partitions"]),
  ("AI vector search in the database", ["ivf_flat", "hnsw_lite", "product_quantization", "mini_vector_database"]),
  ("Spanner: TrueTime and SQL", ["spanner_truetime_commit_wait"]),
  ("Lakehouse: Delta Lake, Spark RDDs", ["delta_lake_transaction_log"]),
  ("Kafka: distributed log", ["kafka_log_model", "record_batch_codec"]),
  ("Redshift: columnar MPP warehouse", ["columnar_encodings", "distributed_query_shuffle_broadcast_join"]),
 ]},
"CMU_15-799_query_optimization": {
 "title": "CMU 15-799 Special Topics: Query Optimization (Spring 2025 schedule)",
 "source": "https://15799.courses.cs.cmu.edu/spring2025/schedule.html",
 "rows": [
  ("01 Intro to Query Optimization", ["query_optimizer"]),
  ("02 IBM System R (Selinger DP)", ["query_optimizer"]),
  ("03 IBM Starburst (rule-based rewrite)", ["query_rewrite_rules_magic_sets"]),
  ("04 Volcano optimizer generator", ["cascades_memo_optimizer"]),
  ("05 Cascades", ["cascades_memo_optimizer"]),
  ("06 Transformations", ["cascades_memo_optimizer", "query_rewrite_rules_magic_sets"]),
  ("07 Join Ordering: Bottom-Up (DPsize, DPhyp, adaptive for large joins)", ["join_enumeration_dpccp_dphyp"]),
  ("08 Join Ordering: Top-Down (MinCut)", ["join_enumeration_dpccp_dphyp"]),
  ("09-10 Parallelisation of the optimizer", ["join_enumeration_dpccp_dphyp"]),
  ("11 Unnesting Queries", ["subquery_unnesting_decorrelation"]),
  ("12 Cost Models: Statistics", ["statistics_histograms_sketches", "hyperloglog", "count_min_sketch"]),
  ("13-14 Cost Models: Cardinality Estimation", ["cardinality_estimation_lab"]),
  ("15 Cost Models: Learned Approaches", ["cardinality_estimation_lab"]),
  ("16 Parametric Query Optimization", ["parametric_query_optimization"]),
  ("17-18 Adaptivity: Feedback and Runtime Processing", ["adaptive_feedback_reoptimization", "adaptive_query_processing_eddies_sip"]),
  ("19 User-Defined Function Optimizations", ["plan_cache_multiplan_trial"]),
  ("20-23 Implementations: Calcite, Orca, MongoDB, Microsoft Fabric", ["cascades_memo_optimizer", "plan_cache_multiplan_trial"]),
 ]},
"time_series_databases": {
 "title": "Time-series databases",
 "source": "CMU Database Group, Time Series Database Lectures (Fall 2017): InfluxDB storage engine, Two Sigma Smooth Storage, QuasarDB, TimescaleDB, Heron; https://db.cs.cmu.edu/seminars/fall2017/ . No dedicated CMU time-series course page was found; topics below follow that series plus standard literature (Gorilla, t-digest).",
 "rows": [
  ("InfluxDB storage engine internals (WAL, cache, TSM files, series index)", ["tsm_time_series_engine"]),
  ("Gorilla compression (delta-of-delta timestamps, XOR floats)", ["timeseries_gorilla"]),
  ("TimescaleDB: hypertables, chunking, chunk exclusion, continuous aggregates", ["hypertables_continuous_aggregates"]),
  ("Cardinality: series churn, HyperLogLog", ["hyperloglog"]),
  ("Downsampling, quantile sketches, anomaly detection", ["timeseries_sketches_downsampling_anomaly"]),
  ("As-of joins, gap filling, out-of-order ingestion", ["asof_joins_gap_filling"]),
  ("Streaming: windows, watermarks, late data (Heron-style)", ["stream_processing_windows"]),
  ("Columnar encodings for numeric series", ["columnar_encodings", "record_batch_codec"]),
  ("Retention, compaction, LSM foundations", ["mini_kv_store", "lsm_compaction_strategies"]),
 ]},
"vector_databases": {
 "title": "Vector databases and similarity search",
 "source": "Topics from the HNSW, DiskANN, IVF/PQ literature and the vector-search lecture in CMU 15-721 (Fall 2024, industry speaker). No dedicated CMU vector-database course page was found.",
 "rows": [
  ("Exact kNN baseline and distance kernels", ["knn_bruteforce", "vector_distance_kernels_simd"]),
  ("IVF (k-means partitioning) and product quantization", ["ivf_flat", "product_quantization"]),
  ("Graph indexes: HNSW and Vamana/DiskANN", ["hnsw_lite", "vamana_diskann_lite"]),
  ("Scalar and binary quantization with re-scoring", ["scalar_binary_quantization_rescoring"]),
  ("Dimensionality reduction: Johnson-Lindenstrauss, PCA", ["dimensionality_reduction_jl_pca"]),
  ("Disk-based IVF (SPANN-style)", ["spann_disk_ivf_lite"]),
  ("Filtered search and hybrid (BM25 + vector) search with rank fusion", ["filtered_ann_hybrid_search", "search_engine"]),
  ("Updates, deletes, segments, persistence", ["vector_index_updates_persistence"]),
  ("Evaluation: recall@k, QPS curves, hardness of datasets", ["ann_evaluation_harness"]),
  ("LSH, MinHash, SimHash", ["minhash_simhash_lsh"]),
  ("A whole mini vector database", ["mini_vector_database"]),
 ]},
"CMU_systems_courses": {
 "title": "Other CMU systems courses (topic maps; syllabi NOT fetched, so treat topic lists as typical, not official)",
 "source": "15-213/513 CS:APP, 15-410 OS Design, 15-418/618 Parallel Architecture, 15-440/640 Distributed Systems, 15-745 Optimizing Compilers, 15-451 Algorithm Design, 15-746 Storage Systems, 15-719 Advanced Cloud",
 "rows": [
  ("15-213 Data lab: bit tricks, IEEE 754 floats", ["bit_tricks", "floating_point_ieee754", "byte_order_keys"]),
  ("15-213 Cache lab and memory hierarchy", ["cache_locality", "cache_simulator", "false_sharing"]),
  ("15-213 Malloc lab", ["size_class_allocator", "bump_arena_allocator", "slab_generational"]),
  ("15-213 Shell lab: processes, signals, job control", ["unix_shell_job_control", "process_pipes_signals"]),
  ("15-213 Linking, loading, ELF/Mach-O", ["elf_linking_loading"]),
  ("15-213 Attack lab (defensive simulation: stack smashing, canaries, ROP)", ["buffer_overflow_rop_simulation"]),
  ("15-213 Proxy lab and network programming", ["tcp_framed_server", "mini_http_server_thread_pool", "reverse_proxy_load_balancer"]),
  ("15-213/18-447 Microarchitecture: branch prediction, pipelines", ["branch_predictor_pipeline_sim"]),
  ("15-410 Kernel: context switch, syscalls, scheduling", ["kernel_context_switch_sim", "scheduler_policies", "syscalls_portable"]),
  ("15-410 Virtual memory: paging, COW fork, ZFOD", ["virtual_memory_sim", "copy_on_write_fork_zfod"]),
  ("15-410 Synchronisation primitives and thread library", ["mutex_condvar_channel", "locks_deadlocks", "atomics_ordering"]),
  ("15-410 File systems", ["toy_filesystem", "file_disk_io_patterns"]),
  ("15-418 Cache coherence, memory consistency", ["cache_coherence_mesi_sim", "memory_ordering_litmus"]),
  ("15-418 GPU/SIMT execution model", ["gpu_simt_warp_model", "simd_intrinsics"]),
  ("15-418 Transactional memory, lock-free", ["software_transactional_memory", "lockfree_stack_aba", "epoch_hazard_pointer_reclamation"]),
  ("15-440 RPC, naming, time, consistency", ["rpc_framework", "clock_sync_and_trace_context", "linearizability_checker"]),
  ("15-440 GFS, MapReduce, Bigtable", ["gfs_lite", "mapreduce_wordcount", "bigtable_lite"]),
  ("15-440 Consensus, replication, CRDTs", ["raft_consensus", "paxos_basic_and_multi", "crdts"]),
  ("15-440 Blockchain basics", ["blockchain_pow"]),
  ("15-745 Dataflow analysis, SSA, register allocation", ["dataflow_analysis_framework", "ssa_register_allocation"]),
  ("15-745 Loop optimisations", ["loop_optimizations"]),
  ("15-745 Alias / points-to analysis", ["alias_analysis_points_to"]),
  ("15-451 Amortised analysis, online algorithms", ["amortized_analysis_online_algorithms"]),
  ("15-451 Randomised and approximation algorithms, LP", ["approximation_randomized_algorithms", "linear_programming_simplex", "max_flow_dinic"]),
  ("15-746 SSDs and flash translation layers", ["ssd_ftl_simulator"]),
  ("15-746 Erasure coding, RAID, deduplication", ["erasure_coding_reed_solomon", "disk_scheduling_raid", "dedup_content_defined_chunking"]),
  ("15-746 Write-optimised structures (LSM, B-epsilon)", ["lsm_compaction_strategies", "b_epsilon_tree"]),
  ("15-719 Cluster scheduling and autoscaling", ["autoscaling_cluster_scheduler", "rate_limiter_lab"]),
 ]},
}
os.makedirs("courses", exist_ok=True)
index = ["# Course maps", "", "Lecture-by-lecture maps from CMU courses to cookbook recipes. ✅ = recipe exists, 🔜 = planned (file name reserved). Regenerated by `python3 courses.py`.", ""]
for slug, c in COURSES.items():
    done = sum(1 for _, ns in c["rows"] for n in ns if n in have)
    total = sum(len(ns) for _, ns in c["rows"])
    out = [f"# {c['title']}", "", f"Source: {c['source']}", "", f"Coverage: {done} of {total} recipe slots exist.", "",
           "| Lecture / topic | Recipes |", "|---|---|"]
    for topic, ns in c["rows"]:
        out.append(f"| {topic} | {mark(ns)} |")
    out += ["", "How to use: read the lecture's papers, attempt the recipe's asserts yourself (see ../HOW_TO_STUDY.md), then compare.", ""]
    open(f"courses/{slug}.md", "w").write("\n".join(out))
    index.append(f"- [{c['title']}]({slug}.md): {done}/{total}")
open("courses/README.md", "w").write("\n".join(index) + "\n")
print("\n".join(index[4:]))
