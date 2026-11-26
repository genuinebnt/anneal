# Other CMU systems courses (topic maps; syllabi NOT fetched, so treat topic lists as typical, not official)

Source: 15-213/513 CS:APP, 15-410 OS Design, 15-418/618 Parallel Architecture, 15-440/640 Distributed Systems, 15-745 Optimizing Compilers, 15-451 Algorithm Design, 15-746 Storage Systems, 15-719 Advanced Cloud

Coverage: 34 of 60 recipe slots exist.

| Lecture / topic | Recipes |
|---|---|
| 15-213 Data lab: bit tricks, IEEE 754 floats | [`bit_tricks`](../examples/algorithms/bit_tricks.rs) ✅<br>`floating_point_ieee754` 🔜<br>[`byte_order_keys`](../examples/perf/byte_order_keys.rs) ✅ |
| 15-213 Cache lab and memory hierarchy | [`cache_locality`](../examples/perf/cache_locality.rs) ✅<br>`cache_simulator` 🔜<br>[`false_sharing`](../examples/perf/false_sharing.rs) ✅ |
| 15-213 Malloc lab | [`size_class_allocator`](../examples/memory/size_class_allocator.rs) ✅<br>[`bump_arena_allocator`](../examples/memory/bump_arena_allocator.rs) ✅<br>[`slab_generational`](../examples/memory/slab_generational.rs) ✅ |
| 15-213 Shell lab: processes, signals, job control | `unix_shell_job_control` 🔜<br>[`process_pipes_signals`](../examples/os_and_storage/process_pipes_signals.rs) ✅ |
| 15-213 Linking, loading, ELF/Mach-O | `elf_linking_loading` 🔜 |
| 15-213 Attack lab (defensive simulation: stack smashing, canaries, ROP) | `buffer_overflow_rop_simulation` 🔜 |
| 15-213 Proxy lab and network programming | [`tcp_framed_server`](../examples/networking/tcp_framed_server.rs) ✅<br>[`mini_http_server_thread_pool`](../examples/capstones/mini_http_server_thread_pool.rs) ✅<br>[`reverse_proxy_load_balancer`](../examples/web/reverse_proxy_load_balancer.rs) ✅ |
| 15-213/18-447 Microarchitecture: branch prediction, pipelines | `branch_predictor_pipeline_sim` 🔜 |
| 15-410 Kernel: context switch, syscalls, scheduling | `kernel_context_switch_sim` 🔜<br>[`scheduler_policies`](../examples/os_and_storage/scheduler_policies.rs) ✅<br>[`syscalls_portable`](../examples/os_and_storage/syscalls_portable.rs) ✅ |
| 15-410 Virtual memory: paging, COW fork, ZFOD | [`virtual_memory_sim`](../examples/os_and_storage/virtual_memory_sim.rs) ✅<br>`copy_on_write_fork_zfod` 🔜 |
| 15-410 Synchronisation primitives and thread library | [`mutex_condvar_channel`](../examples/concurrency/mutex_condvar_channel.rs) ✅<br>[`locks_deadlocks`](../examples/concurrency/locks_deadlocks.rs) ✅<br>[`atomics_ordering`](../examples/concurrency/atomics_ordering.rs) ✅ |
| 15-410 File systems | [`toy_filesystem`](../examples/capstones/toy_filesystem.rs) ✅<br>[`file_disk_io_patterns`](../examples/os_and_storage/file_disk_io_patterns.rs) ✅ |
| 15-418 Cache coherence, memory consistency | `cache_coherence_mesi_sim` 🔜<br>[`memory_ordering_litmus`](../examples/concurrency/memory_ordering_litmus.rs) ✅ |
| 15-418 GPU/SIMT execution model | `gpu_simt_warp_model` 🔜<br>[`simd_intrinsics`](../examples/perf/simd_intrinsics.rs) ✅ |
| 15-418 Transactional memory, lock-free | `software_transactional_memory` 🔜<br>[`lockfree_stack_aba`](../examples/concurrency/lockfree_stack_aba.rs) ✅<br>[`epoch_hazard_pointer_reclamation`](../examples/concurrency/epoch_hazard_pointer_reclamation.rs) ✅ |
| 15-440 RPC, naming, time, consistency | `rpc_framework` 🔜<br>[`clock_sync_and_trace_context`](../examples/distributed/clock_sync_and_trace_context.rs) ✅<br>[`linearizability_checker`](../examples/distributed/linearizability_checker.rs) ✅ |
| 15-440 GFS, MapReduce, Bigtable | `gfs_lite` 🔜<br>`mapreduce_wordcount` 🔜<br>`bigtable_lite` 🔜 |
| 15-440 Consensus, replication, CRDTs | [`raft_consensus`](../examples/distributed/raft_consensus.rs) ✅<br>[`paxos_basic_and_multi`](../examples/distributed/paxos_basic_and_multi.rs) ✅<br>[`crdts`](../examples/distributed/crdts.rs) ✅ |
| 15-440 Blockchain basics | `blockchain_pow` 🔜 |
| 15-745 Dataflow analysis, SSA, register allocation | `dataflow_analysis_framework` 🔜<br>[`ssa_register_allocation`](../examples/compilers_runtimes/ssa_register_allocation.rs) ✅ |
| 15-745 Loop optimisations | `loop_optimizations` 🔜 |
| 15-745 Alias / points-to analysis | `alias_analysis_points_to` 🔜 |
| 15-451 Amortised analysis, online algorithms | `amortized_analysis_online_algorithms` 🔜 |
| 15-451 Randomised and approximation algorithms, LP | `approximation_randomized_algorithms` 🔜<br>`linear_programming_simplex` 🔜<br>[`max_flow_dinic`](../examples/algorithms/max_flow_dinic.rs) ✅ |
| 15-746 SSDs and flash translation layers | `ssd_ftl_simulator` 🔜 |
| 15-746 Erasure coding, RAID, deduplication | `erasure_coding_reed_solomon` 🔜<br>[`disk_scheduling_raid`](../examples/os_and_storage/disk_scheduling_raid.rs) ✅<br>`dedup_content_defined_chunking` 🔜 |
| 15-746 Write-optimised structures (LSM, B-epsilon) | [`lsm_compaction_strategies`](../examples/database/lsm_compaction_strategies.rs) ✅<br>[`b_epsilon_tree`](../examples/data_structures/b_epsilon_tree.rs) ✅ |
| 15-719 Cluster scheduling and autoscaling | `autoscaling_cluster_scheduler` 🔜<br>[`rate_limiter_lab`](../examples/capstones/rate_limiter_lab.rs) ✅ |

How to use: read the lecture's papers, attempt the recipe's asserts yourself (see ../HOW_TO_STUDY.md), then compare.
