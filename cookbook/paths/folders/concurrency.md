# Chapter `concurrency`: suggested reading order

Ordered simple -> intermediate -> advanced (then alphabetical). Skim `Use in:` and `COMMON USES` in each header first. Cross-chapter order is in [the tracks](../tracks/).

| # | Recipe | Level | What it is |
|---|---|---|---|
| 1 | [`crossbeam_select_channels`](../../examples/concurrency/crossbeam_select_channels.rs) | intermediate | crossbeam-channel patterns: rendezvous, select!, timers, pipelines, request/reply, broadcast. |
| 2 | [`joinset_semaphore`](../../examples/concurrency/joinset_semaphore.rs) | intermediate | bounded concurrency with JoinSet + Semaphore, and PROVING the bound. |
| 3 | [`rayon_data_parallelism`](../../examples/concurrency/rayon_data_parallelism.rs) | intermediate | data parallelism with rayon: parallel iterators, fold/reduce, join, scope, thread pools. |
| 4 | [`sharded_map`](../../examples/concurrency/sharded_map.rs) | intermediate | a concurrent HashMap by lock striping: N shards, each `RwLock<HashMap>`. |
| 5 | [`shared_state`](../../examples/concurrency/shared_state.rs) | intermediate | shared state: `Arc<Mutex<_>>` vs an actor that owns the state. |
| 6 | [`threads_channels`](../../examples/concurrency/threads_channels.rs) | intermediate | OS threads: spawn/join, scoped threads, channels, and what `Send`/`Sync` mean. |
| 7 | [`actor_model`](../../examples/concurrency/actor_model.rs) | advanced | a small actor framework on tokio: mailboxes, request/reply, supervision with restarts, registry. |
| 8 | [`atomics_ordering`](../../examples/concurrency/atomics_ordering.rs) | advanced | atomics, and what `Ordering` actually buys you. |
| 9 | [`epoch_hazard_pointer_reclamation`](../../examples/concurrency/epoch_hazard_pointer_reclamation.rs) | advanced | safe memory reclamation for lock-free structures: leak vs hazard pointers vs epochs. |
| 10 | [`lockfree_stack_aba`](../../examples/concurrency/lockfree_stack_aba.rs) | advanced | lock-free Treiber stack, the ABA problem, and tagged heads that defeat it. |
| 11 | [`locks_deadlocks`](../../examples/concurrency/locks_deadlocks.rs) | advanced | Mutex, RwLock, poisoning, lock ordering, try_lock, and Condvar. |
| 12 | [`memory_ordering_litmus`](../../examples/concurrency/memory_ordering_litmus.rs) | advanced | memory-ordering litmus tests with real threads: message passing, store buffering, spinlock, fences. |
| 13 | [`mpmc_bounded_queue`](../../examples/concurrency/mpmc_bounded_queue.rs) | advanced | bounded lock-free MPMC queue (Vyukov style): a ring of sequence-numbered cells. |
| 14 | [`mutex_condvar_channel`](../../examples/concurrency/mutex_condvar_channel.rs) | advanced | build locks, a condvar, a bounded MPMC channel, a semaphore and a barrier from simpler pieces. |
| 15 | [`own_arc_refcount`](../../examples/concurrency/own_arc_refcount.rs) | advanced | your own `Arc<T>` and `Weak<T>` from scratch: atomic refcounts, Release/Acquire on drop, CAS upgrade. |
| 16 | [`seqlock_and_priority_inversion`](../../examples/concurrency/seqlock_and_priority_inversion.rs) | advanced | seqlock for tear-free multi-word reads, and priority inversion vs inheritance vs ceiling. |
| 17 | [`snapshot_cow_rcu`](../../examples/concurrency/snapshot_cow_rcu.rs) | advanced | read-mostly shared state with copy-on-write snapshots (RCU style). |
| 18 | [`spsc_ring_buffer`](../../examples/concurrency/spsc_ring_buffer.rs) | advanced | a lock-free single-producer single-consumer (SPSC) bounded ring buffer. |
| 19 | [`structured_concurrency_cancellation`](../../examples/concurrency/structured_concurrency_cancellation.rs) | advanced | structured concurrency on tokio: CancellationToken tree, TaskTracker, fail-fast Scope, shutdown ordering. |
| 20 | [`thread_pool_work_stealing`](../../examples/concurrency/thread_pool_work_stealing.rs) | advanced | a work-stealing thread pool on crossbeam-deque vs a single shared-queue pool |
