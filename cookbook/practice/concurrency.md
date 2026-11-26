# Practice: concurrency

**How to use this page.** Run the recipe first (`cargo run --release --example <name>`), then work the sections top to bottom: answer "Prove you got it" by experiment before reading the code again, then do one "Try next" task per sitting.
Put your work in a scratch crate or a copy of the example (never in the recipe itself), and keep every task honest with an assertion, a stress loop, or a benchmark. Write the prediction down before you run.
Tasks never come with solutions; if stuck, re-read the recipe's header, then the linked real code. FlowForge stage work (for example the concurrent executor): attempt your stage first, then use these only to compare ideas.

Conventions: [S] small (under 30 min), [M] medium (an evening), [A] advanced (a weekend or more). "Measure it" questions need `--release`; run each measurement 5 times and look at the median and the spread, not one number.

## actor_model

**Prove you got it**
1. Predict: an actor panics while handling message 3 of 10 queued messages. How many of the other 9 are processed, and what is the actor's state afterwards? Check by sending 10 `ask`s and printing replies. Then move the receiver out of the supervisor's `Arc<Mutex<..>>` into the actor itself: what breaks, and why does the mailbox no longer survive restarts?
2. Measure it: send 1M increments to an actor, versus 1M `fetch_add`s on an `AtomicU64` from the same number of tasks. Record msgs/s for both. It would surprise you if the actor were within 10x of the atomic; if it is, find out which part of the actor is cheaper than you assumed.

**Try next**
- [S] Add `Addr::try_ask` that fails fast when the mailbox is full; assert that with capacity 2 and a blocked handler the third call returns `Err` immediately (measure with `Instant`, under 1 ms).
- [M] Add a failure mode: a handler that loops forever. Add a per-message timeout in the supervisor that treats a timeout like a panic (restart with backoff). Test: the restart counter reaches `max_restarts` and senders then see `Err`.
- [A] Re-implement the same bank-account example without tokio using `std::thread` + `crossbeam-channel`, keeping supervision (a thread panic caught through `JoinHandle::join`). Compare lines of code and tail latency of `ask` against the tokio version.

**Use it in**: per-entity state in a service (one actor per user session or device). Same idea in the real [actix](https://github.com/actix/actix) crate; compare its supervision API with your `Policy`.

**Pitfall**: an actor that `ask`s another actor that `ask`s it back deadlocks quietly (both mailboxes wait on each other), and an unbounded mailbox hides the overload until memory runs out.

## atomics_ordering

**Prove you got it**
1. Predict the final value for 8 threads each doing 100k `fetch_add(1, Relaxed)`. Now replace the counter with `load` then `store(v+1)`: predict a range, run 20 times, and explain why the result is never above the true total but often below.
2. In the Release/Acquire publish pattern, change the reader's `Acquire` to `Relaxed`. Predict whether you can observe stale data on your machine, run 1M iterations, then explain why "I could not reproduce it" is not evidence of correctness (hint: the compiler is also allowed to reorder, and x86 hides most reorderings).

**Try next**
- [S] Write a `once_flag` on `AtomicBool` using `compare_exchange`; assert that exactly one of 16 racing threads wins, over 10k rounds.
- [M] Build a "stop flag + result" handoff where the result is a `u64` written non-atomically behind `UnsafeCell`; justify each ordering in a comment, then run it under Miri (if installed) or with `RUSTFLAGS="-Zsanitizer=thread"` on nightly and record the output.
- [A] Port the ordering from a real counter: read `Arc::drop` in std (link below), then write your own "last one out frees" pattern and write down why the decrement needs `Release` and the final path needs an `Acquire` fence.

**Use it in**: lock-free stats counters and shutdown flags in any server. See `fetch_sub(1, Release)` plus the acquire fence in [`Arc` in the std source](https://github.com/rust-lang/rust/blob/master/library/alloc/src/sync.rs).

**Pitfall**: `Relaxed` is fine for a counter nobody uses to decide anything else; the moment another variable's validity depends on the flag, you need Release/Acquire.

## crossbeam_select_channels

**Prove you got it**
1. Predict the output of `bounded(0)` with a sender that sends then prints "sent", and a receiver that sleeps 100 ms before `recv`. When does "sent" print? Check, then change to `bounded(1)` and predict again.
2. Clone a `Receiver` into two consumers and send 100 distinct items: predict how many items each consumer gets, and whether any item is seen twice. Then explain why this is not broadcast.

**Try next**
- [S] Add a `select!` with a `default` arm and a `after(Duration)` arm; assert which wins when a message is already queued.
- [M] Build a 3-stage pipeline (parse, transform, write) with bounded channels of size 4 and a slow last stage; assert the first stage never runs more than 4+4+4 items ahead (track an `AtomicUsize` high-water mark).
- [A] Implement a broadcast channel (every subscriber sees every message) on top of crossbeam, with a slow-subscriber policy (drop-oldest or disconnect). Test with one subscriber that never reads.

**Use it in**: thread pipelines in CLI tools (ripgrep-style worker threads feeding a printer). Real code: [crossbeam-channel](https://github.com/crossbeam-rs/crossbeam/tree/master/crossbeam-channel); std's own channel was rewritten to share its design, see [`std::sync::mpmc`](https://github.com/rust-lang/rust/blob/master/library/std/src/sync/mpmc/mod.rs).

**Pitfall**: dropping every `Sender` is how receivers learn to stop; a stray `Sender` clone held by the main thread makes `for x in rx` hang forever.

## epoch_hazard_pointer_reclamation

**Prove you got it**
1. The recipe reports `peak_pending` per strategy. Predict the order (Naive-leak, hazard pointers, epochs) from smallest to largest peak for a stack with one reader thread that sleeps inside an operation. Run it and explain any mismatch.
2. Make one thread call `enter` and never `leave`. Predict what happens to memory with epochs versus hazard pointers, then verify with the allocator's live-node counter (`check`).

**Try next**
- [S] Change the epoch advance threshold (how many retires before trying to advance) to 1, then 1024; plot or print peak pending vs throughput.
- [M] Add a deliberately buggy strategy (`FreeImmediately`) and write a stress test that makes the `Alloc` checker report a double free or use-after-free within 10 seconds; keep it as a negative test.
- [A] Port the Michael-Scott queue to `crossbeam-epoch` (real crate, `Atomic`/`Owned`/`Shared`, `guard.defer_destroy`) and compare throughput with your hand-written epoch scheme at 1, 4, 8 threads.

**Use it in**: any lock-free map or queue with heap nodes (skip lists, B-link trees). Real code: [crossbeam-epoch](https://github.com/crossbeam-rs/crossbeam/tree/master/crossbeam-epoch) and the skip list built on it in crossbeam-skiplist.

**Pitfall**: a pinned thread that stalls (page fault, preemption, bug) blocks reclamation for everyone with epochs; hazard pointers bound the damage but cost a store plus fence on every protected load.

## joinset_semaphore

**Prove you got it**
1. Predict `max_seen` for 100 tasks, limit 4, each sleeping 10 ms. Change the permit acquisition to happen inside the task AFTER `sleep` instead of before: predict `max_seen` now and check why the bound no longer holds.
2. Measure it: total wall time for 100 tasks x 10 ms at limits 1, 4, 16, 100. Predict the formula first. A result that stops improving before limit 100 means something other than the semaphore limits you; find out what (runtime worker threads? timer granularity?).

**Try next**
- [S] Make the permit RAII-tied to the task result so a panicking task still releases it; assert `available_permits()` returns to the limit afterwards.
- [M] Add a per-task timeout and a "first error cancels the rest" mode (`JoinSet::abort_all`); assert `max_seen <= limit` still holds and that aborted tasks never start after the abort.
- [A] Rewrite as a dependency-ordered scheduler for a generic job graph in a DIFFERENT domain (for example a build system for C files and objects, not FlowForge tasks), with a seeded random graph and an invariant: no job starts before its prerequisites finish. (For FlowForge's own executor stage: attempt your stage first.)

**Use it in**: crawlers and batch API callers that must respect a rate cap. Read how permits work in [tokio's semaphore](https://github.com/tokio-rs/tokio/blob/master/tokio/src/sync/semaphore.rs) and the [JoinSet source](https://github.com/tokio-rs/tokio/blob/master/tokio/src/task/join_set.rs).

**Pitfall**: acquiring the permit inside the spawned task lets millions of tasks pile up (memory) even though only N run; acquire before `spawn` to get backpressure.

## lockfree_stack_aba

**Prove you got it**
1. Predict the stack contents after the hand-replayed ABA schedule in `aba_schedule` for the untagged version, and for the tagged version. Which CAS fails in the tagged one and why?
2. Shrink the tag from its current width to 2 bits. Predict how many pop/push cycles are needed before a wrapped tag makes ABA possible again, then construct that schedule single-threaded.

**Try next**
- [S] Add a `len_hint` and assert after a 4-thread stress run that pushes minus pops equals the final reachable count (`reachable`).
- [M] Add a free list so popped indices are reused (that is what makes ABA likely); write a stress test that fails for the untagged stack (tolerate it being probabilistic: run 200 rounds and count failures) and passes for the tagged one.
- [A] Implement a pointer-based Treiber stack that uses epochs for reclamation (compose with `epoch_hazard_pointer_reclamation`) and benchmark against the index stack at 8 threads.

**Use it in**: object pools and allocator free lists. Real code: crossbeam's [`ArrayQueue`/`SegQueue`](https://github.com/crossbeam-rs/crossbeam/tree/master/crossbeam-queue) avoid ABA with sequence numbers rather than tags.

**Pitfall**: a tag is a probabilistic fix (it wraps); the real fix is to make reuse impossible while a stale reader exists (reclamation schemes).

## locks_deadlocks

**Prove you got it**
1. Predict whether two threads doing `transfer(a,b)` and `transfer(b,a)` in a loop deadlock without lock ordering. Run 1 second with a watchdog thread that prints "deadlock suspected" if progress stops. Then apply ordering by account id and predict again.
2. Panic while holding a `Mutex` guard, then lock again: predict the result type, then recover with `into_inner()` and decide when ignoring poison is wrong.

**Try next**
- [S] Replace `lock()` with `try_lock()` in `transfer` and add a bounded retry with backoff; assert no deadlock across 100k transfers and total money conserved.
- [M] Add a `RwLock` cache and write a test that shows writer starvation or reader-heavy slowdown; record which side your platform's lock favours.
- [A] Build a lock-order checker: a wrapper `OrderedMutex<T, const RANK: u32>` plus a thread-local stack that panics in debug when ranks are acquired out of order; prove it by provoking one deliberate violation in a test.

**Use it in**: any in-memory ledger or cache. Real code: [parking_lot](https://github.com/Amanieu/parking_lot) ships a `deadlock_detection` feature; read how it finds cycles.

**Pitfall**: a `Condvar` wait without a loop around the predicate loses wakeups and wakes spuriously.

## memory_ordering_litmus

**Prove you got it**
1. Predict how many iterations out of 1M show `r1==0 && r2==0` in store buffering with `SeqCst`, and with Release/Acquire. Run both and report counts; on Apple Silicon versus x86 they differ.
2. In message passing, change the flag store to `Relaxed`: predict whether stale data appears on your CPU, and say why a negative result proves nothing.

**Try next**
- [S] Add the IRIW (independent reads of independent writes) litmus test; list which orderings allow the odd outcome.
- [M] Add a load-buffering test (`r1=y; x=1` vs `r2=x; y=1`) and record outcomes under Relaxed; compare with documented model behaviour.
- [A] Take the `SpinLock` from the recipe, weaken its orderings to the minimum, and prove with `loom` ([tokio-rs/loom](https://github.com/tokio-rs/loom)) that mutual exclusion still holds, and that weakening one more ordering makes loom find a violation.

**Use it in**: reviewing unsafe synchronisation in any lock-free crate. The ordering contract is documented in the [`atomic` module source](https://github.com/rust-lang/rust/blob/master/library/core/src/sync/atomic.rs).

**Pitfall**: passing a litmus test on x86 (strong memory model) says little about ARM; test on both or use loom.

## mpmc_bounded_queue

**Prove you got it**
1. Predict the `seq` value of cell 0 after one push and one pop in a capacity-8 queue. Print it by adding a debug accessor, and explain the `pos + capacity` rule.
2. Measure it: 4 producers + 4 consumers, 10M items, versus `Mutex<VecDeque>` and `crossbeam_channel::bounded`. Predict ranking. If your queue is not faster than the mutex at 1+1 threads, that is normal; find the thread count where it crosses over.

**Try next**
- [S] Add `len()` as an approximate count and assert it never exceeds capacity during a stress run.
- [M] Add a `push_timeout` using spin-then-yield; check that a full queue returns `Err(item)` and the item is not dropped (use the `Counted` drop tracker).
- [A] Make it blocking-capable by parking waiting threads (eventcount idea) so an idle consumer burns zero CPU; verify with a test measuring `getrusage` CPU time while idle for 1 second.

**Use it in**: log and metrics pipelines between worker threads. Real code: [crossbeam-queue ArrayQueue](https://github.com/crossbeam-rs/crossbeam/tree/master/crossbeam-queue).

**Pitfall**: the queue is lock-free but not wait-free, and a producer that stalls between claiming a slot and publishing it blocks the consumer of that slot.

## mutex_condvar_channel

**Prove you got it**
1. Predict which lock (`TicketLock`, `TtasLock`, `ParkMutex`, `std::sync::Mutex`) wins at 1, 4 and 16 threads on your core count and why the order flips when threads exceed cores. Measure and compare.
2. Predict what the `ParkMutex` unlock path does when state is 1 versus 2, and verify by counting `park`/`unpark` calls in an uncontended loop (should be zero).

**Try next**
- [S] Add `try_lock` to each lock and assert it never succeeds while another thread holds the lock.
- [M] Add a lock-fairness metric: max wait observed per thread across 1M acquisitions; show the ticket lock has the smallest spread and TTAS the largest.
- [A] Implement a futex-style mutex directly on Linux futex or macOS `os_unfair_lock` through FFI (feature-gated per OS) and compare with `ParkMutex`; read [std's futex mutex](https://github.com/rust-lang/rust/blob/master/library/std/src/sys/sync/mutex/futex.rs) afterwards and write down three differences.

**Use it in**: understanding std and [parking_lot](https://github.com/Amanieu/parking_lot) internals; spin crates such as [spin-rs](https://github.com/mvdnes/spin-rs) for `no_std`.

**Pitfall**: spinlocks under oversubscription (more threads than cores) can be catastrophically slower because the lock holder is descheduled while others spin.

## own_arc_refcount

**Prove you got it**
1. Predict `strong_count` and `weak_count` after: create, clone, downgrade, drop the original, upgrade. Print each step.
2. Predict whether `T`'s destructor runs when the last strong drops while a `MyWeak` still exists, and whether the allocation is freed. Verify with a drop-counter type and an allocation counter (compose with `counting_allocator` in perf).

**Try next**
- [S] Add `make_mut` (clone-on-write when shared); assert it clones exactly when `strong_count > 1`.
- [M] Add cyclic construction (`new_cyclic`-style) so a node can hold a weak pointer to itself; test it does not leak by counting drops.
- [A] Run under Miri if available and write the three UB-class bugs you would expect if you used `Relaxed` on the decrement; demonstrate one with loom.

**Use it in**: shared ownership in any threaded program; real code: [`alloc::sync::Arc`](https://github.com/rust-lang/rust/blob/master/library/alloc/src/sync.rs).

**Pitfall**: reference cycles leak with `Arc`; parent-to-child strong, child-to-parent weak.

## rayon_data_parallelism

**Prove you got it**
1. Predict the result of a parallel `reduce` using subtraction as the operator on 1..=1000, run it 10 times, and explain why it varies (associativity).
2. Measure it: speedup of `par_iter().sum()` over `iter().sum()` for 1k, 100k, 10M elements. A speedup below 1 at small sizes is expected; find the crossover size.

**Try next**
- [S] Replace `par_mergesort` threshold with 100, 4096, 100_000 and print time; find the knee.
- [M] Add a deterministic parallel word count with `fold` + `reduce` into `HashMap`; assert equality with the sequential version on 20 seeded random inputs.
- [A] Parallelise a different CPU-bound task you own (image blur, k-means step, or a Monte Carlo pi) with rayon, measure scaling at 1/2/4/8 threads via `ThreadPoolBuilder`, and plot efficiency; identify the first thing that stops scaling.

**Use it in**: batch transforms in CLIs and build tools; real code: [rayon](https://github.com/rayon-rs/rayon), with ripgrep's parallel directory walk as a contrast (it uses its own pool).

**Pitfall**: parallelism over tiny work items or over a shared `Mutex` can be slower than serial; nested `par_iter` inside a lock can deadlock.

## seqlock_and_priority_inversion

**Prove you got it**
1. Predict the count of torn reads from the naive plain-struct seqlock and from `SeqLock` in `seqlock_stress`; run both. If the naive version shows zero, explain why that is still UB.
2. In the priority-inversion simulation, predict the high-priority task's finish time without inheritance and with inheritance/ceiling before reading the `Outcome`.

**Try next**
- [S] Make `Quote` bigger (4 words, 8 words) and measure reader retry rate at fixed writer frequency.
- [M] Add a second writer and show the single-writer assumption breaks; then add a writer mutex and re-run the stress test with zero torn reads.
- [A] Build a lock-free "latest value" market-data cell for a struct of 6 fields with `SeqLock`, and compare with `RwLock<Arc<T>>` from `snapshot_cow_rcu` under 1 writer / 7 readers.

**Use it in**: clocks and stats snapshots read at high frequency; the Linux kernel's `seqlock_t`. For the priority-inheritance half, see how real-time kernels handle it (Mars Pathfinder is the classic story).

**Pitfall**: readers must copy using atomic or volatile word reads; plain copies racing a writer are UB even if you throw the result away.

## sharded_map

**Prove you got it**
1. Predict the throughput ratio between 1 shard and 64 shards at 8 threads on a 90 percent read workload, then measure. Surprise: the gap is small on read-heavy loads; find out why (RwLock reader count atomics still contend).
2. Predict what `update_or_insert` guarantees that `get` followed by `insert` does not. Break it: implement the two-step version and show lost increments with 8 threads.

**Try next**
- [S] Change the shard count to a non-power-of-two and a power of two; measure whether `% n` versus `& mask` matters.
- [M] Add `iter_snapshot()` that takes all shard locks in a fixed order, and a test with concurrent writers asserting it never deadlocks.
- [A] Compare against `dashmap` ([dashmap](https://github.com/xacrimon/dashmap)) and a `RwLock<HashMap>` under zipfian-skewed keys; show where a hot key defeats sharding and try a fix (per-key atomic counters).

**Use it in**: session tables and caches. Real code: dashmap uses the same striping.

**Pitfall**: sharding by a poor hash (or hashing in the caller then re-hashing) clusters keys on a few shards; measure the shard occupancy.

## shared_state

**Prove you got it**
1. Predict whether holding a `std::sync::Mutex` guard across an `.await` compiles when the future is spawned; read the error and explain it in your own words.
2. Predict how a long critical section in pattern A versus a slow handler in pattern B affect other callers.

**Try next**
- [S] Add a command to pattern B that returns a snapshot of the whole state; assert it is internally consistent after concurrent updates.
- [M] Add timeouts to both: a `try_lock` with deadline for A, a `oneshot` with `timeout` for B; compare the failure behaviour.
- [A] Implement a rate-limited cache (reads dominate, background refresh) in both styles and benchmark p50 and p99 latency at 64 concurrent tasks.

**Use it in**: any service that holds in-memory state; real code: [mini-redis](https://github.com/tokio-rs/mini-redis) uses a mutex around its db with a background purge task.

**Pitfall**: actor mailboxes are queues, so a slow message blocks everything behind it.

## snapshot_cow_rcu

**Prove you got it**
1. Predict torn reads from `Snapshot` (0) and `RacyConfig` (nonzero) in your run; run it and report the counts.
2. A reader holds a snapshot for 1 s while the writer replaces it 1000 times. Predict how many versions are alive at once, then verify with a drop counter on `Config`.

**Try next**
- [S] Add `update` that retries if another writer swapped in between (compare-and-swap on the epoch).
- [M] Add many readers holding old versions and show memory growth (versions alive); add a cap that makes writers wait.
- [A] Replace the `RwLock<Arc<T>>` with an `arc-swap`-style atomic pointer plus your own reclamation (compose with `epoch_hazard_pointer_reclamation`) and benchmark reads at 16 threads; compare with the [left-right](https://github.com/jonhoo/left-right) crate's approach.

**Use it in**: hot-reloadable config and routing tables in proxies (see `reverse_proxy_load_balancer` in the web chapter).

**Pitfall**: the readers' cost is the read lock plus an atomic refcount bump; at high core counts that cache line becomes the bottleneck.

## spsc_ring_buffer

**Prove you got it**
1. Predict what breaks if `head` and `tail` share a cache line (remove `CachePadded`) at 10M items; measure the throughput change.
2. Predict the exact `head`/`tail` values after pushing 5 and popping 5 in a capacity-4 queue of wrapping counters, then verify by printing.

**Try next**
- [S] Add `push_slice`/`pop_slice` batching and measure items/s at batch sizes 1, 16, 256.
- [M] Add a `Drop` test with a tracked payload: items left in the queue must be dropped exactly once when both ends vanish.
- [A] Build a bidirectional pipeline (two rings) between a reader thread and writer thread that moves a file through compress and checksum stages; assert byte-exact output and measure MB/s.

**Use it in**: audio and logging hand-off (see `io_uring_model` in the os chapter for the same ring protocol). Real code: the bounded rings in [crossbeam-queue](https://github.com/crossbeam-rs/crossbeam/tree/master/crossbeam-queue) (MPMC, a superset of what you wrote) and the submission/completion rings in [io-uring](https://github.com/tokio-rs/io-uring).

**Pitfall**: SPSC means exactly one producer thread: two producers silently corrupt it (make `Producer` `!Clone` and check `Send` only).

## structured_concurrency_cancellation

**Prove you got it**
1. Predict the order of events when a parent token is cancelled while two children are mid-work, then verify with timestamps. Cancel only a child: does the parent notice?
2. Predict what `TaskTracker::wait` does if a tracked task panics, then check.

**Try next**
- [S] Add `child_with_timeout` that cancels itself after a duration; assert it does not cancel siblings.
- [M] Add a fail-fast `Scope` test: with 10 tasks and task 3 failing, assert the other nine observe cancellation within 50 ms.
- [A] Port a small server (the accept loop from `tcp_framed_server` in networking) to use the token tree and TaskTracker for a graceful shutdown that drains in-flight connections within a deadline, then force-kills the rest.

**Use it in**: servers that must shut down cleanly. Real code: [`CancellationToken`](https://github.com/tokio-rs/tokio/blob/master/tokio-util/src/sync/cancellation_token.rs) and [TaskTracker](https://github.com/tokio-rs/tokio/tree/master/tokio-util/src/task).

**Pitfall**: cancelling a token is a request, not a kill; a task that never awaits never sees it.

## thread_pool_work_stealing

**Prove you got it**
1. Predict the speedup of work stealing versus the shared-queue pool for recursive fib(35) with tiny jobs on your core count; measure both.
2. Predict what happens to the stealing pool if one worker runs a very long job: do others still make progress?

**Try next**
- [S] Count steals per worker and print the distribution.
- [M] Add a `scope`-style API so jobs may borrow stack data; then break the safety (unsafe lifetime trick) in a test that Miri or the compiler must reject.
- [A] Add parking: idle workers sleep after N failed steals and wake on spawn; assert zero CPU when idle for 1 s.

**Use it in**: rayon and tokio's multi-thread scheduler ([runtime](https://github.com/tokio-rs/tokio/tree/master/tokio/src/runtime)); deques from [crossbeam-deque](https://github.com/crossbeam-rs/crossbeam/tree/master/crossbeam-deque).

**Pitfall**: a pool whose workers block on a `join` of another job can deadlock unless the blocked worker helps run jobs (see `help_one`).

## threads_channels

**Prove you got it**
1. Predict the compile error for moving an `Rc<i32>` into `thread::spawn`; produce it and read the `Send` message. Then do it with `Arc` and `RefCell`: what changes?
2. Predict thread-scope behaviour: borrow a local `Vec` in `thread::scope` threads; remove `scope` and use `spawn`: which error and why.

**Try next**
- [S] Compute a parallel sum with scoped threads over chunks and assert equality with the sequential sum for random inputs.
- [M] Add a worker pool with a result channel and an error channel; assert no result is lost when a worker panics.
- [A] Implement a parallel grep over a directory tree with a bounded work queue and one printer thread; assert output equals the single-thread run after sorting.

**Use it in**: CPU-bound background workers; real code: ripgrep's parallel walker in [BurntSushi/ripgrep](https://github.com/BurntSushi/ripgrep).

**Pitfall**: `join()` returns `Err` for a panicked thread; ignoring it hides lost work.

## Chapter projects

### Small (about 2 h): stress-testing harness for a lock
Combine `mutex_condvar_channel`, `atomics_ordering`, `perf/bench_harness`.
- M1: write a generic `fn stress<L: Lock>()` where `L` is any of your locks; invariant: a non-atomic counter protected by it equals threads x iterations.
- M2: add a fairness report (max wait per thread) and a throughput table at 1/2/4/8/16 threads using the median-of-5 rule from `bench_harness`.
- Accept: counter correct for all locks over 20 seeds; one deliberately broken lock (unlock with `Relaxed` plus a plain read) is caught by loom or by a failing stress run.
- Stretch: add `parking_lot::Mutex` and `std::sync::Mutex` and rank them.

### Intermediate (a weekend): bounded work-queue service
Combine `mpmc_bounded_queue`, `thread_pool_work_stealing`, `structured_concurrency_cancellation`, plus `networking/tcp_framed_server` and `distributed/token_bucket`.
- M1: a TCP server that frames jobs, pushes them into your MPMC queue, and a worker pool consumes them.
- M2: token-bucket admission in front of the queue; full queue returns an explicit "busy" frame, never blocks the acceptor.
- M3: graceful shutdown with a deadline using the token tree.
- Accept: a load generator sends 100k jobs; invariants: every accepted job completes exactly once, rejected plus accepted equals sent, queue length never exceeds capacity, shutdown finishes in under the deadline.
- Stretch: p50/p99 latency report; compare with crossbeam-channel as the queue.

### Advanced (a week): a concurrent in-memory KV store with snapshots
Combine `sharded_map`, `snapshot_cow_rcu`, `seqlock_and_priority_inversion`, `epoch_hazard_pointer_reclamation`, `os_and_storage/wal_append_log` and `perf/false_sharing`.
- M1: sharded map with per-shard stats counters (padded).
- M2: consistent point-in-time `snapshot()` using copy-on-write per shard or a global epoch.
- M3: a WAL for durability and crash recovery tests (kill the process mid-write via a child process).
- M4: replace one shard lock with an epoch-reclaimed lock-free structure.
- Accept: a linearizability-style check (single-key register model) over 1M seeded random ops with 8 threads; recovery test reproduces the exact state after 100 injected crashes; throughput scaling chart 1 to 16 threads.
- Stretch: compare against dashmap; write up where each loses.
