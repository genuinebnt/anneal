#!/usr/bin/env python3
"""Generate learning paths from the recipe files (run by index.py; safe to run alone).

  paths/README.md            how to choose
  paths/tracks/*.md          ordered curricula ("learn X from zero")
  paths/building/*.md        "I am building X: read these, in this order, at these moments"
  paths/folders/<chapter>.md every chapter in teaching order (level, then dependency hints), generated from headers

Files are never moved: recipe names, course maps and cross references stay stable. Every name used below is checked against
the real files; a typo fails the run. A name in PLANNED renders as a 🔜 placeholder (not written yet).
"""
import re, sys, pathlib

ROOT = pathlib.Path(__file__).parent
EX = ROOT / "examples"
OUT = ROOT / "paths"

info = {}  # name -> (chapter, level, title)
for f in sorted(EX.glob("*/*.rs")):
    if f.parent.name == "scratch":
        continue
    level, title = "?", f.stem
    for line in f.read_text().splitlines()[:12]:
        m = re.match(r"//! RECIPE \d+ \([^)]*\): (.*)", line)
        if m:
            title = m.group(1).strip()
        m = re.match(r"//! LEVEL: *(\w+)", line)
        if m:
            level = m.group(1)
    info[f.stem] = (f.parent.name, level, title)
crates = {d.name for d in (ROOT / "crates").iterdir() if (d / "Cargo.toml").exists()}
PLANNED = {  # recipes referenced by a path but not written yet
    "mini_vector_database", "timeseries_gorilla", "tsm_time_series_engine", "stream_processing_windows",
    "cache_simulator", "mapreduce_wordcount", "rpc_framework", "dataflow_analysis_framework",
    "vector_distance_kernels_simd", "scalar_binary_quantization_rescoring", "filtered_ann_hybrid_search",
    "cache_coherence_mesi_sim", "erasure_coding_reed_solomon", "gfs_lite", "bigtable_lite",
}
errors = []

def link(name, depth):
    up = "../" * depth
    if name in info:
        ch, lv, title = info[name]
        return f"[`{name}`]({up}examples/{ch}/{name}.rs) · {lv}", title
    if name in crates:
        return f"[`{name}`]({up}crates/{name}/) · crate", "multi-file crate"
    if name in PLANNED:
        return f"`{name}` 🔜", "not written yet"
    errors.append(name)
    return f"`{name}` ❓", ""

def render_steps(steps, depth):
    out = []
    for i, (heading, goal, items) in enumerate(steps, 1):
        out.append(f"### Step {i}: {heading}\n\n**Goal:** {goal}\n")
        out.append("| # | Recipe | Why here |\n|---|---|---|")
        for j, (name, why) in enumerate(items, 1):
            l, _ = link(name, depth)
            out.append(f"| {i}.{j} | {l} | {why} |")
        out.append("")
    return "\n".join(out)

LOOP = ("**How to work each step:** read the header, predict the asserts, blank the file and rebuild it from memory "
        "(`./study.sh NAME`), then mutate it. Details in [HOW_TO_STUDY.md](../../HOW_TO_STUDY.md). "
        "Checkpoint = you can explain the step's goal in five lines without looking.\n")

TRACKS = {
"01_rust_foundations": ("Rust foundations: from syntax to idiomatic code", "About 3-4 weeks at 6 h/week.", [
 ("Values, ownership, strings", "Stop fighting the borrow checker; know where data lives.", [
   ("strings", "String vs &str vs bytes is the first wall everyone hits."), ("bytes", "Byte slices and the useful methods."),
   ("borrowck_patterns", "The handful of shapes the borrow checker accepts and rejects."), ("drop_raii", "Ownership ends at Drop; guards depend on it."),
   ("mem_functions", "take/replace/swap: how to move out of &mut safely.")]),
 ("Collections, iterators, closures", "Write loops as iterator pipelines; pick the right collection.", [
   ("collections_tour", "Vec, HashMap, BTreeMap, VecDeque, BinaryHeap: when each wins."), ("iterators", "The Iterator contract."),
   ("iterator_adapters", "map/filter/fold/windows/chunks/zip combinations."), ("closures_hof", "Fn/FnMut/FnOnce and returning closures.")]),
 ("Traits and generics", "Design APIs with traits instead of copy-paste.", [
   ("common_traits", "Debug, Clone, Default, PartialOrd, Hash, From: derive vs hand-write."), ("conversions", "From/Into/TryFrom/AsRef."),
   ("generics_bounds", "Bounds and where clauses."), ("generics_lifetimes_structs", "Structs and methods holding references."),
   ("dyn_vs_generic", "Static vs dynamic dispatch; object safety."), ("implementing_std_traits", "Make your type feel like std.")]),
 ("Errors", "Design the failure paths before the happy path.", [
   ("option_result_toolbox", "Combinators and ?."), ("error_enum", "A typed error enum with From."), ("anyhow_context", "Applications vs libraries."),
   ("error_chain_retryable", "Sources, classification, retryable vs fatal.")]),
 ("Structure and polish", "Code a reviewer would merge.", [
   ("modules_visibility", "pub(crate), re-exports, layout."), ("newtype_ids", "Type-safe IDs (you need this in FlowForge)."),
   ("pattern_matching", "Exhaustive matching and bindings."), ("smart_pointers", "Box/Rc/Arc/RefCell/Cow in practice."),
   ("library_api_polish", "API Guidelines checklist applied."), ("testing_patterns", "Unit, property and golden tests."),
   ("rust_ownership_borrowing_qa", "Self-test: close the file and answer first.")]),
]),
"02_memory_unsafe": ("Memory, layout and unsafe", "Only after track 01. About 3 weeks.", [
 ("Layout and arenas (safe)", "See where bytes go before using unsafe.", [
   ("data_layout", "Size, alignment, padding."), ("struct_packing", "Field order and packing."), ("arena_ids", "Vec + index instead of pointers."),
   ("slab_generational", "Reusing slots safely with generation counters."), ("bump_arena_allocator", "Allocation as pointer bump.")]),
 ("Unsafe core", "Know the contracts the compiler stops checking.", [
   ("unsafe_basics", "The five unsafe powers and SAFETY comments."), ("unsafecell_cells", "Interior mutability foundations."),
   ("maybeuninit_fixedvec", "Uninitialised memory without UB."), ("nonnull_linked_list", "Raw pointers in a data structure."),
   ("phantomdata_typed", "Variance and ownership markers."), ("pin_projection", "Why Pin exists."), ("repr_layout_ffi", "repr(C), packed, transparent.")]),
 ("Applying it", "Allocators and boundaries.", [
   ("size_class_allocator", "A malloc in miniature."), ("garbage_collectors", "Tracing collectors."), ("ffi_c_interop", "Calling and being called by C."),
   ("cb-cdylib-c-abi", "A real cdylib crate."), ("rust_unsafe_memory_layout_qa", "Self-test.")]),
]),
"03_concurrency_async": ("Concurrency and async", "About 4 weeks.", [
 ("Threads and locks", "Share state without data races.", [
   ("threads_channels", "Spawn, join, channels."), ("shared_state", "Arc<Mutex<T>> and its limits."), ("mutex_condvar_channel", "Blocking primitives from first principles."),
   ("locks_deadlocks", "Lock ordering and detection."), ("sharded_map", "Reduce contention by sharding."), ("rayon_data_parallelism", "Data parallelism.")]),
 ("Atomics and memory ordering", "Lock-free thinking.", [
   ("atomics_ordering", "Relaxed/Acquire/Release/SeqCst."), ("memory_ordering_litmus", "Litmus tests that fail on purpose."),
   ("spsc_ring_buffer", "The simplest lock-free structure."), ("mpmc_bounded_queue", "Sequence-number queue."), ("lockfree_stack_aba", "The ABA problem."),
   ("epoch_hazard_pointer_reclamation", "Safe memory reclamation."), ("seqlock_and_priority_inversion", "Optimistic reads.")]),
 ("Schedulers and structured concurrency", "Build the engine underneath.", [
   ("thread_pool_work_stealing", "Work stealing."), ("actor_model", "Message-passing design."), ("structured_concurrency_cancellation", "Scoped lifetimes for tasks."),
   ("joinset_semaphore", "Bounded fan-out (needed for FlowForge's limit N).")]),
 ("Async", "Understand futures before using them.", [
   ("futures_by_hand", "Poll, Pending, Waker."), ("waker_executor", "A tiny executor."), ("async_patterns", "Common async shapes."),
   ("select_cancel", "select! and what cancellation drops."), ("async_cancel_safety", "Cancel-safe vs not."), ("bounded_channel", "Backpressure."),
   ("worker_pool", "Pool with graceful shutdown."), ("graceful_shutdown", "Draining on SIGTERM."), ("mini_async_runtime", "Capstone: runtime from scratch."),
   ("rust_async_qa", "Self-test.")]),
]),
"04_data_structures_algorithms": ("Data structures and algorithms in Rust", "About 6 weeks; interview files double as timed practice.", [
 ("Linear and hashing", "The workhorses.", [
   ("heap_patterns", "Heaps, tie-breaking, top-k."), ("ring_buffer_zoo", "Circular buffers."), ("hash_functions", "Why hash choice matters."),
   ("open_addressing_map", "Build a hash map."), ("lru_cache", "Hash map + list."), ("cache_policy_zoo", "FIFO/LRU/LFU/CLOCK/2Q/ARC/SIEVE compared.")]),
 ("Trees and tries", "Ordered and prefix structures.", [
   ("binary_trees_in_rust", "Ownership of recursive types."), ("trie_radix", "Tries and radix trees."), ("treap_interval", "Balanced and interval trees."),
   ("segment_fenwick", "Range queries."), ("skiplist", "Probabilistic ordered list."), ("bplus_tree", "Database-style trees.")]),
 ("Graphs and DP", "Modelling problems.", [
   ("index_graph", "Vec-index graphs (the Rust way)."), ("csr_graph", "Compressed sparse rows."), ("dijkstra", "Shortest paths."), ("union_find", "Disjoint sets."),
   ("dp_memoization", "Memo vs tabulation."), ("dp_zoo", "Recognise the DP shape."), ("max_flow_dinic", "Flows and matching.")]),
 ("Strings and search", "Text structure.", [
   ("string_algos", "KMP, Z, rolling hash."), ("aho_corasick", "Multi-pattern search."), ("tries_core_questions", "Interview trie set."), ("tries_advanced_questions", "Word Search II, XOR tries."),
   ("bloom_filter", "Probabilistic membership.")]),
 ("Interview practice", "Retrieval practice: timer, scratch file, oracle.", [
   ("arrays_strings_hashmap_questions", "Arrays, strings, hashing."), ("stacks_queues_heaps_questions", "Stacks, queues, heaps."),
   ("linked_lists_in_rust", "Why linked lists are hard in Rust."), ("graphs_and_union_find_questions", "Graph questions."),
   ("dp_and_backtracking_questions", "DP and backtracking."), ("implement_std_from_scratch", "Implement Vec, Rc, HashMap.")]),
]),
"05_performance": ("Performance engineering", "Measure first. About 3 weeks.", [
 ("Measure", "Never optimise unmeasured.", [("bench_harness", "A trustworthy micro-benchmark."), ("counting_allocator", "Count allocations."), ("perf_checklist", "Order of investigation.")]),
 ("Memory behaviour", "Most speed-ups live here.", [("cache_locality", "Row vs column, strides."), ("false_sharing", "Cache lines across threads."), ("data_layout", "SoA vs AoS."),
   ("small_string_inline", "Avoid allocations."), ("byte_order_keys", "Order-preserving encodings.")]),
 ("CPU behaviour", "Branches and SIMD.", [("branches_vectorization", "Help the autovectoriser."), ("simd_intrinsics", "Explicit SIMD."), ("matmul_optimization", "The classic ladder."),
   ("inlining_codegen", "Read the assembly."), ("fast_hashing", "Hasher choice."), ("bufwriter", "I/O batching.")]),
]),
"06_databases_cmu": ("Databases: a CMU 15-445 -> 15-721 -> 15-799 path", "About 12-16 weeks. Course maps in ../../courses/ list every lecture; this is the build order.", [
 ("Storage and buffering", "Pages, files, memory.", [
   ("slotted_pages_heap_file", "Pages and record ids."), ("storage_models_nsm_dsm_pax", "Row vs column layouts."), ("buffer_pool_clock", "Eviction basics."),
   ("lru_k_replacer_disk_scheduler", "LRU-K and a disk scheduler."), ("wal_append_log", "Logging basics."), ("columnar_encodings", "Dictionary, RLE, bit-packing.")]),
 ("Indexes", "Ordered, hash and modern structures.", [
   ("bplus_tree", "The baseline."), ("btree_bulk_load_prefix_compression", "Engineering a B+tree."), ("hash_and_bitmap_indexes", "Hash and bitmap."),
   ("adaptive_radix_tree", "ART."), ("optimistic_lock_coupling_btree", "Concurrent B+trees."), ("bw_tree_latch_free", "Latch-free index."),
   ("b_epsilon_tree", "Write-optimised."), ("lsm_compaction_strategies", "LSM trade-offs."), ("learned_index_rmi", "Learned indexes."), ("gist_generalized_search_tree", "Extensible indexing.")]),
 ("Query execution", "Operators and parallelism.", [
   ("relational_algebra_engine", "Algebra and rewrites."), ("volcano_vectorized_push", "Execution models."), ("join_algorithms", "Join algorithms."),
   ("external_sort_hash_aggregation", "Sort and aggregate beyond memory."), ("radix_partitioned_hash_join", "Cache-conscious joins."), ("bitweaving_bit_sliced_scans", "Scans on codes."),
   ("sql_window_functions_ctes", "Windows and recursion."), ("parallel_query_morsel_exchange", "Morsel-driven parallelism.")]),
 ("Transactions and recovery", "Correctness under concurrency and crashes.", [
   ("transaction_theory_serializability", "Theory first."), ("lock_manager_deadlock", "2PL."), ("timestamp_ordering_occ", "T/O and OCC."), ("mvcc_snapshot_isolation", "MVCC basics."),
   ("mvcc_version_storage_gc", "MVCC design space."), ("logging_schemes_group_commit", "Logging schemes."), ("aries_recovery", "ARIES.")]),
 ("Query optimisation", "From rules to cost-based search.", [
   ("statistics_histograms_sketches", "Statistics."), ("cardinality_estimation_lab", "Where estimates fail."), ("query_rewrite_rules_magic_sets", "Rewrites."),
   ("subquery_unnesting_decorrelation", "Decorrelation."), ("join_enumeration_dpccp_dphyp", "Join ordering."), ("cascades_memo_optimizer", "Cascades."),
   ("parametric_query_optimization", "Parametric plans."), ("adaptive_feedback_reoptimization", "Feedback."), ("adaptive_query_processing_eddies_sip", "Runtime adaptivity."),
   ("plan_cache_multiplan_trial", "Plan caching."), ("query_optimizer", "Capstone."), ("tiny_sql_engine", "Capstone: end to end.")]),
 ("Distributed and cloud databases", "Beyond one machine.", [
   ("distributed_query_shuffle_broadcast_join", "Shuffle vs broadcast."), ("dremel_nested_columnar_shredding", "Nested columnar."), ("cloud_warehouse_micro_partitions", "Warehouse design."),
   ("delta_lake_transaction_log", "Lakehouse logs."), ("spanner_truetime_commit_wait", "External consistency.")]),
]),
"07_distributed_systems": ("Distributed systems", "About 6 weeks.", [
 ("Time and ordering", "No global clock.", [("vector_clocks_hlc", "Logical and hybrid clocks."), ("clock_sync_and_trace_context", "Clocks and tracing."), ("chandy_lamport_snapshot", "Consistent snapshots.")]),
 ("Partitioning and replication", "Spread and copy data.", [("consistent_hashing", "Placement."), ("hash_partitioning", "Partition schemes."), ("quorum_replication_dynamo", "Quorums."), ("crdts", "Merge without coordination."), ("operational_transform", "Collaborative editing.")]),
 ("Consensus and transactions", "Agree despite failure.", [("paxos_basic_and_multi", "Paxos."), ("raft_consensus", "Raft."), ("two_phase_commit", "2PC."), ("saga_orchestrator", "Sagas."), ("linearizability_checker", "Check your claims."), ("swim_gossip", "Failure detection.")]),
 ("Messaging and resilience", "What services actually use.", [("kafka_log_model", "Log-structured messaging."), ("record_batch_codec", "Batch formats."), ("retry_backoff_jitter", "Retries."), ("token_bucket", "Rate limiting."), ("lease_fencing_sim", "Leases and fencing (FlowForge)."), ("event_sourced_ledger", "Event sourcing.")]),
]),
"08_compilers_languages": ("Compilers and languages", "About 6-8 weeks. The flagship crate comes last.", [
 ("Lexing and parsing", "Text to trees.", [("zero_copy_lexer", "Tokens without allocation."), ("expression_parsing_three_ways", "Precedence climbing, Pratt, combinators."), ("pratt_parser", "Pratt in depth."),
   ("parser_combinators", "Composable parsers."), ("json_parser", "A real grammar."), ("diagnostics_spans", "Errors that point at source.")]),
 ("Execution", "Run a program.", [("bytecode_vm", "A stack VM."), ("toy_language_interpreter", "End-to-end interpreter."), ("wasm_binary_interpreter", "A real binary format."), ("hindley_milner_inference", "Type inference."),
   ("ssa_register_allocation", "Middle and back end."), ("parser_generators_lr_peg_earley", "Parser theory.")]),
 ("The full pipeline", "Read after building a toy.", [("cb-compiler", "Pebble: parser, types, diagnostics, (stage 2) MIR and VM."), ("cb-compiler-tools", "Formatter, highlighter, LSP.")]),
]),
"09_networking_web": ("Networking and web services", "About 4 weeks.", [
 ("Wire formats", "Bytes on the wire.", [("binary_format_parsing", "Parse safely."), ("resp_protocol", "A text/binary protocol."), ("http1_parser", "HTTP framing."), ("websocket_frames", "Framing and masking."), ("dns_message_codec", "Compression pointers.")]),
 ("Sockets and event loops", "How servers wait.", [("tcp_framed_server", "Length-prefixed protocols."), ("kqueue_epoll_raw", "Readiness APIs."), ("mio_event_loop", "An event loop."), ("udp_sockets_reliable", "Reliability on UDP."), ("tcp_congestion_control_sim", "Congestion control.")]),
 ("Services", "Production concerns.", [("axum_router_state", "Routing and state."), ("axum_errors", "Error responses."), ("axum_middleware", "Layers."), ("rest_api_patterns", "Pagination, versions."),
   ("axum_idempotency", "Safe retries."), ("auth_basic_bearer_jwt", "AuthN."), ("http_client_resilience", "Timeouts/retries."), ("reverse_proxy_load_balancer", "Proxying."), ("webhooks_signatures", "Verify callbacks."), ("job_service_axum", "Capstone.")]),
]),
"10_search_vectors": ("Search and vector similarity", "About 4 weeks.", [
 ("Text search", "Index words and substrings.", [("trigram_index", "Substring search."), ("autocomplete_engine", "Prefix completion."), ("bloom_filter", "Skip lookups."), ("search_engine", "Capstone."), ("typeahead_service", "Capstone.")]),
 ("Vector search", "Similarity at scale.", [("knn_bruteforce", "Baseline and recall."), ("ivf_flat", "Partitioning."), ("product_quantization", "Compression."), ("hnsw_lite", "Graph index."), ("vamana_diskann_lite", "Disk-friendly graph."), ("minhash_simhash_lsh", "Near duplicates."), ("mini_vector_database", "Capstone (planned).")]),
]),
"11_os_storage_systems": ("Operating-system and storage internals", "About 4 weeks.", [
 ("Files and I/O", "What read() really does.", [("file_disk_io_patterns", "Sequential vs random."), ("buffered_io_internals", "Buffering."), ("mmap_files", "Memory mapping."), ("file_locking_atomic_replace", "Crash-safe updates."), ("io_uring_model", "Async I/O."), ("zero_copy_sendfile_splice", "Avoid copies.")]),
 ("Memory and scheduling", "Virtual memory and CPUs.", [("virtual_memory_sim", "Paging."), ("scheduler_policies", "Scheduling."), ("process_pipes_signals", "Processes."), ("timing_wheel", "Timers."), ("disk_scheduling_raid", "Disks.")]),
 ("Build one", "Capstones.", [("toy_filesystem", "A filesystem."), ("mini_kv_store", "A storage engine."), ("mini_redis", "A server and persistence."), ("mini_http_server_thread_pool", "A server.")]),
]),
}

BUILD = {
"workflow_engine": ("A workflow / job orchestration engine (FlowForge)", "Read at the moment indicated; attempt your own version first.", [
 ("Stage 1-2: parse, count, bad input", "Load JSON, report errors without panics.", [("serde_defaults", "Optional fields and defaults."), ("serde_custom", "Custom (de)serialisation."), ("error_enum", "Typed errors."), ("clap_cli", "A CLI with exit codes."), ("diagnostics_spans", "Pointing at the bad token.")]),
 ("Stage 3-4: schema, ids, dependencies", "Validate before running anything.", [("newtype_ids", "TaskId newtype."), ("collections_tour", "HashMap vs BTreeMap (determinism)."), ("arena_ids", "Index instead of String lookups."), ("option_result_toolbox", "Chained validation.")]),
 ("Stage 5-8: graph order and scale", "Attempt the algorithm first; then compare.", [("index_graph", "Vec-index graph."), ("csr_graph", "Dense and large graphs."), ("heap_patterns", "Deterministic tie-breaking."), ("shrinking", "Shrink failing random DAGs."), ("cache_locality", "Why layout matters at 1M nodes."), ("bench_harness", "Measure.")]),
 ("Executors", "Sequential, then concurrent with limit N.", [("threads_channels", "Basics."), ("joinset_semaphore", "Limit N."), ("thread_pool_work_stealing", "Pool."), ("structured_concurrency_cancellation", "Cancel on failure."), ("retry_backoff_jitter", "Task retries."), ("graceful_shutdown", "Drain.")]),
 ("Persistence and API", "Survive restarts; expose HTTP.", [("wal_append_log", "Durable log."), ("event_sourced_ledger", "Event sourcing."), ("lease_fencing_sim", "Prevent double-run."), ("saga_orchestrator", "Compensation."), ("axum_router_state", "REST."), ("axum_idempotency", "Idempotent submit."), ("job_service_axum", "Capstone."), ("tracing_logs", "Observability.")]),
]),
"rest_service": ("A production-style REST service", "", [
 ("Skeleton", "Routing, state, errors.", [("axum_router_state", "State and handlers."), ("axum_errors", "One error type to responses."), ("axum_middleware", "Auth, logging, limits."), ("tracing_logs", "Structured logs.")]),
 ("API quality", "What clients feel.", [("rest_api_patterns", "Pagination, filtering."), ("axum_idempotency", "Retries."), ("http_caching_conditional", "ETags."), ("cors_security_headers", "Headers."), ("axum_sse_replay", "Streaming.")]),
 ("Security", "Who is calling.", [("auth_basic_bearer_jwt", "Tokens."), ("cookies_sessions_csrf", "Sessions."), ("oauth2_pkce_flow", "OAuth."), ("webhooks_signatures", "Signed callbacks."), ("constant_time_side_channels", "Comparisons."), ("secure_random_and_passwords", "Hashing.")]),
 ("Resilience", "Failure of dependencies.", [("http_client_resilience", "Timeouts, retries."), ("retry_backoff_jitter", "Backoff."), ("token_bucket", "Limit."), ("rate_limiter_lab", "Compare limiters."), ("graceful_shutdown", "Deploy safely.")]),
]),
"database_engine": ("A database / storage engine", "Build in this order; each layer needs the one before.", [
 ("Durable bytes", "Pages and a log.", [("file_disk_io_patterns", "I/O costs."), ("slotted_pages_heap_file", "Pages."), ("wal_append_log", "WAL."), ("file_locking_atomic_replace", "Atomic file updates."), ("buffer_pool_clock", "Cache pages.")]),
 ("An index", "Find things.", [("bplus_tree", "Ordered index."), ("lsm_compaction_strategies", "Or an LSM."), ("bloom_filter", "Skip SSTables."), ("mini_kv_store", "Capstone.")]),
 ("Transactions", "Concurrency and crashes.", [("lock_manager_deadlock", "Locks."), ("mvcc_snapshot_isolation", "Snapshots."), ("aries_recovery", "Recovery."), ("linearizability_checker", "Test your claims.")]),
 ("A query layer", "SQL on top.", [("expression_parsing_three_ways", "Parse."), ("volcano_vectorized_push", "Execute."), ("join_algorithms", "Join."), ("query_optimizer", "Optimise."), ("tiny_sql_engine", "End to end.")]),
]),
"search_engine": ("A search engine or autocomplete", "", [
 ("Indexing", "Postings and tokens.", [("trie_radix", "Prefix structures."), ("trigram_index", "Substring."), ("bitset_rank_select_roaring", "Compressed postings."), ("encodings_codecs", "Varint and delta.")]),
 ("Ranking and fuzzy", "Order results.", [("heap_patterns", "Top-k."), ("autocomplete_engine", "Completion."), ("string_algos", "Edit distance/automata."), ("search_engine", "Capstone."), ("typeahead_service", "Service version.")]),
 ("Scale", "Many shards.", [("consistent_hashing", "Shard."), ("bloom_filter", "Skip shards."), ("sharded_map", "Concurrent updates.")]),
]),
"vector_search": ("A vector database", "The capstone is planned; these are its parts.", [
 ("Distances and baseline", "Exact first.", [("knn_bruteforce", "Baseline."), ("simd_intrinsics", "Fast distance kernels."), ("vector_distance_kernels_simd", "Distance kernels (planned).")]),
 ("Indexes", "Trade recall for speed.", [("ivf_flat", "Partitioning."), ("product_quantization", "Compression."), ("hnsw_lite", "Graph."), ("vamana_diskann_lite", "Disk graph.")]),
 ("Database features", "Make it a product.", [("scalar_binary_quantization_rescoring", "Rescoring (planned)."), ("filtered_ann_hybrid_search", "Filters and hybrid (planned)."), ("mini_vector_database", "Capstone (planned)."), ("trigram_index", "Hybrid keyword leg.")]),
]),
"message_queue": ("A message queue / Kafka-like log", "", [
 ("The log", "Append-only storage.", [("wal_append_log", "Segments."), ("kafka_log_model", "Partitions, offsets, consumer groups."), ("record_batch_codec", "Batch format."), ("encodings_codecs", "Varints.")]),
 ("Delivery", "Semantics.", [("retry_backoff_jitter", "Retries."), ("saga_orchestrator", "Exactly-once illusions."), ("bounded_channel", "Backpressure.")]),
 ("Cluster", "Replicate.", [("raft_consensus", "Replicated log."), ("quorum_replication_dynamo", "Quorums."), ("consistent_hashing", "Partition placement.")]),
]),
"cache_or_kv_server": ("A cache / Redis-like server", "", [
 ("Protocol and loop", "Talk to clients.", [("resp_protocol", "RESP."), ("kqueue_epoll_raw", "Readiness."), ("mio_event_loop", "Event loop."), ("tcp_framed_server", "Framing.")]),
 ("Data and eviction", "What lives in memory.", [("open_addressing_map", "Hash table."), ("skiplist", "Sorted sets."), ("hyperloglog", "Cardinality."), ("lru_cache", "Eviction."), ("cache_policy_zoo", "Policies."), ("timing_wheel", "TTL expiry.")]),
 ("Durability", "Survive restarts.", [("wal_append_log", "AOF."), ("snapshot_cow_rcu", "Snapshots."), ("mini_redis", "Capstone.")]),
]),
"language_or_dsl": ("A language, DSL or compiler", "", [
 ("Front end", "Syntax.", [("zero_copy_lexer", "Lexer."), ("pratt_parser", "Expressions."), ("edge_dsl_parser", "A small DSL."), ("diagnostics_spans", "Good errors.")]),
 ("Middle", "Meaning.", [("hindley_milner_inference", "Types."), ("ssa_register_allocation", "IR and registers.")]),
 ("Back end", "Run it.", [("bytecode_vm", "VM."), ("toy_language_interpreter", "Interpreter."), ("wasm_binary_interpreter", "Target WASM.")]),
 ("Tooling", "Developer experience.", [("cb-compiler", "Reference pipeline."), ("cb-compiler-tools", "Formatter and LSP."), ("cb-macros", "Procedural macros.")]),
]),
"distributed_lock_leader": ("Leader election, locks and coordination", "", [
 ("Why it is hard", "Clocks and pauses.", [("vector_clocks_hlc", "Time."), ("lease_fencing_sim", "Fencing tokens."), ("linearizability_checker", "What 'correct' means.")]),
 ("Consensus", "Build on it, don't reinvent.", [("raft_consensus", "Raft."), ("paxos_basic_and_multi", "Paxos."), ("swim_gossip", "Membership.")]),
]),
"cli_tool": ("A command-line tool", "", [
 ("Interface", "Args and output.", [("clap_cli", "Parsing."), ("error_enum", "Exit codes."), ("anyhow_context", "Context for users."), ("io_files_paths", "Files.")]),
 ("Quality", "Reliable.", [("testing_patterns", "Golden tests."), ("tracing_logs", "Logs."), ("shrinking", "Property tests."), ("library_api_polish", "Doc and API.")]),
 ("Speed", "If needed.", [("bufwriter", "Output."), ("rayon_data_parallelism", "Parallel."), ("bench_harness", "Measure.")]),
]),
"async_runtime_server": ("A high-performance async network server", "", [
 ("Foundations", "Futures.", [("futures_by_hand", "Poll."), ("waker_executor", "Executor."), ("async_cancel_safety", "Cancellation.")]),
 ("Server", "Many connections.", [("mini_async_runtime", "Runtime."), ("mini_http_server_thread_pool", "Threaded baseline."), ("http1_parser", "Protocol."), ("async_streams_concurrency", "Streams."), ("graceful_shutdown", "Shutdown."), ("tower_service_layer", "Middleware.")]),
 ("Tuning", "Measure and fix.", [("kqueue_epoll_raw", "Event loops."), ("io_uring_model", "Next-gen I/O."), ("false_sharing", "Contention."), ("bench_harness", "Benchmark.")]),
]),
}

ORDER = {"simple": 0, "intermediate": 1, "advanced": 2}

def write(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)

def main():
    for key, (title, blurb, steps) in TRACKS.items():
        body = f"# Track: {title}\n\n{blurb}\n\n{LOOP}\n{render_steps(steps, 2)}"
        write(OUT / "tracks" / f"{key}.md", body)
    for key, (title, blurb, steps) in BUILD.items():
        body = (f"# Building: {title}\n\n{blurb}\n\n"
                "Use this when you have a real project in front of you. For each stage: **attempt your design first**, then read the "
                "listed recipes to compare, steal what is better and note what you missed.\n\n" + render_steps(steps, 2))
        write(OUT / "building" / f"{key}.md", body)
    chapters = {}
    for name, (ch, lv, title) in info.items():
        chapters.setdefault(ch, []).append((ORDER.get(lv, 3), name, lv, title))
    for ch, rows in chapters.items():
        rows.sort()
        lines = [f"# Chapter `{ch}`: suggested reading order\n",
                 "Ordered simple -> intermediate -> advanced (then alphabetical). Skim `Use in:` and `COMMON USES` in each header first. "
                 "Cross-chapter order is in [the tracks](../tracks/).\n",
                 "| # | Recipe | Level | What it is |", "|---|---|---|---|"]
        for i, (_, name, lv, title) in enumerate(rows, 1):
            lines.append(f"| {i} | [`{name}`](../../examples/{ch}/{name}.rs) | {lv} | {title} |")
        write(OUT / "folders" / f"{ch}.md", "\n".join(lines) + "\n")
    idx = ["# Learning paths\n",
           "Three ways in. Pick by your situation:\n",
           "1. **I want to learn a subject from zero** -> a *track* (ordered curriculum, with a goal per step).",
           "2. **I am building something right now** -> a *building guide* (read at the moment you hit that sub-problem).",
           "3. **I am looking at one folder** -> its *folder guide* (teaching order inside the chapter, generated from recipe headers).\n",
           "Recipes are never moved, so names and links elsewhere stay valid. 🔜 marks a recipe that is planned but not written yet.\n",
           "## Tracks\n"]
    for key, (title, blurb, steps) in TRACKS.items():
        n = sum(len(s[2]) for s in steps)
        idx.append(f"- [{title}](tracks/{key}.md) ({n} recipes)")
    idx.append("\n## Building guides\n")
    for key, (title, blurb, steps) in BUILD.items():
        n = sum(len(s[2]) for s in steps)
        idx.append(f"- [{title}](building/{key}.md) ({n} recipes)")
    idx.append("\n## Folder guides\n")
    for ch in sorted(chapters):
        idx.append(f"- [`{ch}`](folders/{ch}.md) ({len(chapters[ch])} recipes)")
    idx.append("\n## Suggested overall order\n\n"
               "`01_rust_foundations` -> `04_data_structures_algorithms` (alongside FlowForge stages 1-8) -> `03_concurrency_async` -> `05_performance` -> "
               "then branch by interest: databases `06`, distributed `07`, compilers `08`, web `09`, search/vectors `10`, systems `11`. "
               "`02_memory_unsafe` only after foundations and some concurrency.\n")
    write(OUT / "README.md", "\n".join(idx))
    if errors:
        print("UNKNOWN RECIPE NAMES:", sorted(set(errors)))
        sys.exit(1)
    print(f"paths: {len(TRACKS)} tracks, {len(BUILD)} building guides, {len(chapters)} folder guides")

main()
