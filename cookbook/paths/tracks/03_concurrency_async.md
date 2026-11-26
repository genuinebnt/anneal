# Track: Concurrency and async

About 4 weeks.

**How to work each step:** read the header, predict the asserts, blank the file and rebuild it from memory (`./study.sh NAME`), then mutate it. Details in [HOW_TO_STUDY.md](../../HOW_TO_STUDY.md). Checkpoint = you can explain the step's goal in five lines without looking.

### Step 1: Threads and locks

**Goal:** Share state without data races.

| # | Recipe | Why here |
|---|---|---|
| 1.1 | [`threads_channels`](../../examples/concurrency/threads_channels.rs) · intermediate | Spawn, join, channels. |
| 1.2 | [`shared_state`](../../examples/concurrency/shared_state.rs) · intermediate | Arc<Mutex<T>> and its limits. |
| 1.3 | [`mutex_condvar_channel`](../../examples/concurrency/mutex_condvar_channel.rs) · advanced | Blocking primitives from first principles. |
| 1.4 | [`locks_deadlocks`](../../examples/concurrency/locks_deadlocks.rs) · advanced | Lock ordering and detection. |
| 1.5 | [`sharded_map`](../../examples/concurrency/sharded_map.rs) · intermediate | Reduce contention by sharding. |
| 1.6 | [`rayon_data_parallelism`](../../examples/concurrency/rayon_data_parallelism.rs) · intermediate | Data parallelism. |

### Step 2: Atomics and memory ordering

**Goal:** Lock-free thinking.

| # | Recipe | Why here |
|---|---|---|
| 2.1 | [`atomics_ordering`](../../examples/concurrency/atomics_ordering.rs) · advanced | Relaxed/Acquire/Release/SeqCst. |
| 2.2 | [`memory_ordering_litmus`](../../examples/concurrency/memory_ordering_litmus.rs) · advanced | Litmus tests that fail on purpose. |
| 2.3 | [`spsc_ring_buffer`](../../examples/concurrency/spsc_ring_buffer.rs) · advanced | The simplest lock-free structure. |
| 2.4 | [`mpmc_bounded_queue`](../../examples/concurrency/mpmc_bounded_queue.rs) · advanced | Sequence-number queue. |
| 2.5 | [`lockfree_stack_aba`](../../examples/concurrency/lockfree_stack_aba.rs) · advanced | The ABA problem. |
| 2.6 | [`epoch_hazard_pointer_reclamation`](../../examples/concurrency/epoch_hazard_pointer_reclamation.rs) · advanced | Safe memory reclamation. |
| 2.7 | [`seqlock_and_priority_inversion`](../../examples/concurrency/seqlock_and_priority_inversion.rs) · advanced | Optimistic reads. |

### Step 3: Schedulers and structured concurrency

**Goal:** Build the engine underneath.

| # | Recipe | Why here |
|---|---|---|
| 3.1 | [`thread_pool_work_stealing`](../../examples/concurrency/thread_pool_work_stealing.rs) · advanced | Work stealing. |
| 3.2 | [`actor_model`](../../examples/concurrency/actor_model.rs) · advanced | Message-passing design. |
| 3.3 | [`structured_concurrency_cancellation`](../../examples/concurrency/structured_concurrency_cancellation.rs) · advanced | Scoped lifetimes for tasks. |
| 3.4 | [`joinset_semaphore`](../../examples/concurrency/joinset_semaphore.rs) · intermediate | Bounded fan-out (needed for FlowForge's limit N). |

### Step 4: Async

**Goal:** Understand futures before using them.

| # | Recipe | Why here |
|---|---|---|
| 4.1 | [`futures_by_hand`](../../examples/async/futures_by_hand.rs) · advanced | Poll, Pending, Waker. |
| 4.2 | [`waker_executor`](../../examples/async/waker_executor.rs) · advanced | A tiny executor. |
| 4.3 | [`async_patterns`](../../examples/async/async_patterns.rs) · intermediate | Common async shapes. |
| 4.4 | [`select_cancel`](../../examples/async/select_cancel.rs) · intermediate | select! and what cancellation drops. |
| 4.5 | [`async_cancel_safety`](../../examples/async/async_cancel_safety.rs) · advanced | Cancel-safe vs not. |
| 4.6 | [`bounded_channel`](../../examples/async/bounded_channel.rs) · intermediate | Backpressure. |
| 4.7 | [`worker_pool`](../../examples/async/worker_pool.rs) · advanced | Pool with graceful shutdown. |
| 4.8 | [`graceful_shutdown`](../../examples/async/graceful_shutdown.rs) · intermediate | Draining on SIGTERM. |
| 4.9 | [`mini_async_runtime`](../../examples/capstones/mini_async_runtime.rs) · capstone | Capstone: runtime from scratch. |
| 4.10 | [`rust_async_qa`](../../examples/interview/rust_async_qa.rs) · advanced | Self-test. |
