# Concurrency

Threads, locks, atomics and memory ordering, lock-free structures, channels, thread pools and data parallelism.

Run any recipe from the `cookbook/` directory: `cargo run --release --example <name>` (the name is the file name without `.rs`). Each file starts with a header: what it shows, `LEVEL`, `COMMON USES` and `Read:` links.

20 examples.

| Example | What it shows | Level | Common uses | Crates |
|---|---|---|---|---|
| [`joinset_semaphore`](joinset_semaphore.rs) | bounded concurrency with JoinSet + Semaphore, and PROVING the bound | intermediate | bounded concurrency in crawlers, batch processors and API clients: Semaphore + JoinSet with a provable limit. | tokio |
| [`atomics_ordering`](atomics_ordering.rs) | atomics, and what `Ordering` actually buys you | advanced | counters, flags, lock-free structures, publishing config to other threads without a mutex. | std only |
| [`shared_state`](shared_state.rs) | shared state: `Arc<Mutex<_>>` vs an actor that owns the state | intermediate | caches, registries and schedulers: mutex for tiny sections, actor task when invariants span several fields. | tokio |
| [`threads_channels`](threads_channels.rs) | OS threads: spawn/join, scoped threads, channels, and what `Send`/`Sync` mean | intermediate | CPU-bound parallel work, background workers, understanding what async runtimes build on. | std only |
| [`locks_deadlocks`](locks_deadlocks.rs) | Mutex, RwLock, poisoning, lock ordering, try_lock, and Condvar | advanced | shared caches and accounts: lock ordering, poisoning policy, Condvar signalling. | std only |
| [`memory_ordering_litmus`](memory_ordering_litmus.rs) | memory-ordering litmus tests with real threads: message passing, store buffering, spinlock, fences | advanced | mutexes/spinlocks, reference counts (Arc uses Release on decrement + Acquire fence), `Once`, SPSC queues (spsc_ring_buffer), seqlocks. | std only |
| [`spsc_ring_buffer`](spsc_ring_buffer.rs) | a lock-free single-producer single-consumer (SPSC) bounded ring buffer | advanced | the `rtrb` and `ringbuf` crates, Linux kernel `kfifo`, audio callback queues, LMAX-Disruptor-style pipelines. | std only |
| [`mpmc_bounded_queue`](mpmc_bounded_queue.rs) | bounded lock-free MPMC queue (Vyukov style): a ring of sequence-numbered cells | advanced | bounded channels (back-pressure instead of unbounded memory growth), thread-pool task queues, | std only |
| [`lockfree_stack_aba`](lockfree_stack_aba.rs) | lock-free Treiber stack, the ABA problem, and tagged heads that defeat it | advanced | lock-free allocator free lists and object pools, task freelists in runtimes; crossbeam-epoch's | std only |
| [`sharded_map`](sharded_map.rs) | a concurrent HashMap by lock striping: N shards, each `RwLock<HashMap>` | intermediate | in-process caches, connection/session registries, metrics registries keyed by name, deduplication sets. | std only |
| [`snapshot_cow_rcu`](snapshot_cow_rcu.rs) | read-mostly shared state with copy-on-write snapshots (RCU style) | advanced | config reload on SIGHUP, service discovery tables, compiled regex/rule sets swapped atomically, Rust | std only |
| [`thread_pool_work_stealing`](thread_pool_work_stealing.rs) | a work-stealing thread pool on crossbeam-deque vs a single shared-queue pool | advanced | data-parallel CPU work (sorting, parsing, image tiles, parallel search), the executors under rayon | crossbeam-deque, rayon |
| [`own_arc_refcount`](own_arc_refcount.rs) | your own `Arc<T>` and `Weak<T>` from scratch: atomic refcounts, Release/Acquire on drop, CAS upgrade | advanced | this is how std::sync::Arc, the `triomphe` crate (no weak count) and tokio's task refcounts work; | std only |
| [`mutex_condvar_channel`](mutex_condvar_channel.rs) | build locks, a condvar, a bounded MPMC channel, a semaphore and a barrier from simpler pieces | advanced | spinlocks in kernels (Linux uses ticket/queued spinlocks), parking_lot-style small mutexes, work queues and | std only |
| [`actor_model`](actor_model.rs) | a small actor framework on tokio: mailboxes, request/reply, supervision with restarts, registry | advanced | Erlang/OTP supervision trees (the original "let it crash" design), actix and ractor are Rust actor | tokio |
| [`rayon_data_parallelism`](rayon_data_parallelism.rs) | data parallelism with rayon: parallel iterators, fold/reduce, join, scope, thread pools | intermediate | parallel builds/linters over many files, ripgrep-style file scanning pipelines, image processing, Monte Carlo, | rayon |
| [`epoch_hazard_pointer_reclamation`](epoch_hazard_pointer_reclamation.rs) | safe memory reclamation for lock-free structures: leak vs hazard pointers vs epochs | advanced | crossbeam-epoch backs crossbeam's SkipMap/deque/queue; hazard pointers are in folly (Meta), the C++26 | crossbeam-utils |
| [`seqlock_and_priority_inversion`](seqlock_and_priority_inversion.rs) | seqlock for tear-free multi-word reads, and priority inversion vs inheritance vs ceiling | advanced | seqlocks: the Linux kernel (`seqlock_t`, used for jiffies/xtime and VDSO time reads), game and trading | std only |
| [`structured_concurrency_cancellation`](structured_concurrency_cancellation.rs) | structured concurrency on tokio: CancellationToken tree, TaskTracker, fail-fast Scope, shutdown ordering | advanced | HTTP servers (stop the listener, let in-flight requests finish, then force close), Kubernetes style | tokio |
| [`crossbeam_select_channels`](crossbeam_select_channels.rs) | crossbeam-channel patterns: rendezvous, select!, timers, pipelines, request/reply, broadcast | intermediate | thread-based pipelines (parse -> transform -> write), worker pools with graceful stop by dropping | crossbeam-channel |
