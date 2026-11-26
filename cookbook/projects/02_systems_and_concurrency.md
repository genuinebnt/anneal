# 02. Systems and concurrency

Threads, atomics, async, schedulers and pipelines. Recipe stems are `folder/stem` under `examples/`. 🔜 = planned, not written yet.

Testing rule for this theme: concurrency bugs hide. Every project needs (a) a deterministic single-thread oracle, (b) a stress test with at least 8 threads x 100,000 ops, (c) `loom` or Miri where it applies. Never assert on wall-clock timing except as a generous upper bound.

---

## 2.1 `boundq`: a bounded blocking queue with a stress harness  `S`

**Goal.** Build `BoundedQueue<T>` (capacity N, `push` blocks when full, `pop` blocks when empty, `close()` wakes everyone) using `Mutex` + two `Condvar`s, then a harness that proves no item is lost or duplicated.
**Learn.** Condvar loops (spurious wakeups), close semantics, lock scope, poisoning, test harness design.
**Read before.** `concurrency/mutex_condvar_channel`, `concurrency/threads_channels`. **During.** `concurrency/locks_deadlocks`, `concurrency/mpmc_bounded_queue`.
**Milestones.**
1. Single producer/consumer, FIFO order preserved.
2. N producers x M consumers; `close()` drains remaining items, then `pop` returns `None`.
3. `push_timeout(Duration)` and `try_push`.
4. Harness: 4 producers push the integers `0..250_000` tagged by producer id; consumers collect; assert the multiset equals the input and per-producer order is preserved.
5. Benchmark vs `std::sync::mpsc::sync_channel` and `crossbeam::channel::bounded`.
**Acceptance.** Capacity 1, 2, 64 all pass 50 repeated runs; no test takes >5 s; a deliberately broken version (`if` instead of `while` around `wait`) is caught by your harness within 100 runs.
**Data.** The integers themselves plus a producer tag; a `Drop`-counting payload to detect double-drop.
**Write-up.** Why two condvars instead of one? What happens to blocked producers on `close()`?
**Stretch.** `pop_batch(max)` that returns up to 64 items per lock acquisition; measure the throughput gain.
**Resembles.** [crossbeam-channel](https://github.com/crossbeam-rs/crossbeam), Go channels.

## 2.2 `shardmap`: a sharded concurrent map vs `RwLock<HashMap>`  `S`

**Goal.** Implement `ShardMap<K,V>` with 16/64/256 shards and measure it against a global `Mutex`, `RwLock`, and `dashmap`-style designs under read-heavy, write-heavy and Zipfian workloads.
**Learn.** Lock striping, hashing to a shard, false sharing, contention measurement, workload modelling.
**Read before.** `concurrency/sharded_map`, `concurrency/shared_state`. **During.** `perf/false_sharing`, `perf/bench_harness`, `data_structures/hash_functions`.
**Milestones.**
1. Correctness: get/insert/remove/len match `HashMap` oracle under single thread (2,000 random ops).
2. Multi-thread correctness: 8 threads each own a key range; final map equals union.
3. Pad shards to 64 bytes; show the difference with and without padding.
4. Zipf(s=1.1) key generator, 1M ops, 90/10 and 50/50 read/write mixes.
5. Plot throughput vs thread count 1,2,4,8,16 (CSV is enough).
**Acceptance.** Written conclusion with numbers: at which thread count and mix does sharding beat RwLock by at least 2x? If it never does on your machine, explain why.
**Data.** Zipfian generator you write (inverse CDF table over 100,000 keys); uniform as control.
**Write-up.** What breaks if a closure passed to `get_or_insert_with` locks another shard? How do you implement `len()` cheaply?
**Stretch.** Resizable shard count with rehash under a global epoch.
**Resembles.** [dashmap/crossbeam](https://github.com/crossbeam-rs/crossbeam) designs, Java `ConcurrentHashMap`.

## 2.3 `poolrs`: a work-stealing thread pool with scoped tasks  `M`

**Goal.** A thread pool with per-worker deques, stealing, `spawn`, `scope` (borrow stack data safely) and graceful `shutdown` that drains work, plus a `parallel_sum` built on it.
**Learn.** Work stealing, `Arc`/`Weak` in workers, panic handling in tasks, scope lifetimes (why `scope` is sound), backoff and parking.
**Read before.** `concurrency/thread_pool_work_stealing`, `concurrency/structured_concurrency_cancellation`. **During.** `concurrency/rayon_data_parallelism`, `concurrency/epoch_hazard_pointer_reclamation`, `async/graceful_shutdown`.
**Milestones.**
1. Fixed pool, single shared queue, `spawn` + `JoinHandle<T>`.
2. Per-worker queues; idle workers steal half from a random victim.
3. A panicking task does not kill the worker; `join` returns `Err`.
4. `scope(|s| s.spawn(|| borrows_local_vec))` with a compile-fail test for escaping borrows.
5. `shutdown(Duration)` finishes queued work then joins; further `spawn` returns `Err`.
**Acceptance.** 1M tiny tasks complete with exactly 1M executions (atomic counter); recursive fib(30) spawn tree finishes without deadlock on a 2-thread pool; parallel_sum over 100M u64 is at least 3x faster than sequential on 4+ cores (report the measured value, do not assert).
**Data.** Skewed task sizes: 99% of 1 us, 1% of 10 ms, to show stealing matters.
**Write-up.** How does a worker waiting on a child task avoid deadlock on a bounded pool? Why do you need `'scope` and not `'static`?
**Stretch.** Priority classes; a `par_iter().map().sum()` adapter.
**Resembles.** [rayon](https://github.com/rayon-rs/rayon), [tokio runtime](https://github.com/tokio-rs/tokio) scheduler.

## 2.4 `cronq`: a job scheduler with retries and a timing wheel  `M`

**Goal.** An in-process scheduler: one-off, interval and cron-expression jobs, retry with exponential backoff and jitter, per-job timeout, concurrency cap, and a persisted job table that survives restart (JSON file with atomic replace).
**Learn.** Timing wheels vs binary heap, cron parsing, clock abstraction (testability), idempotent restart, atomic file replace.
**Read before.** `os_and_storage/timing_wheel`, `distributed/retry_backoff_jitter`, `os_and_storage/file_locking_atomic_replace`. **During.** `concurrency/joinset_semaphore`, `patterns/state_machine`, `dev_tools/testing_patterns`.
**Milestones.**
1. Inject a `Clock` trait; implement `FakeClock` so tests advance time instantly.
2. 5-field cron parser (`*/15 9-17 * * 1-5`) with `next_after(t)`; 200 hand-checked cases incl. DST-free UTC, Feb 29, month-end.
3. Heap-based scheduler, then swap in a hierarchical timing wheel (3 levels of 64 slots); results identical.
4. Retry policy: base 100 ms, factor 2, cap 30 s, full jitter, max 5 attempts; job states `Pending/Running/Succeeded/Failed/DeadLetter`.
5. Persist on every state change; kill -9 mid-run and restart: running jobs become `Pending` with `attempt+1`.
**Acceptance.** Simulated 30 days in under 2 s using `FakeClock`; 10,000 interval jobs fire exactly the expected number of times (computed by closed form); concurrency cap of 4 never exceeded (atomic high-water mark).
**Data.** Cron cases from the POSIX crontab man page plus your own 50 edge cases; seeded failure injector that makes 30% of attempts fail.
**Write-up.** At-least-once vs at-most-once: which do you give and how does a job opt into the other? What does restart do to a job whose side effect ran but whose state write did not?
**Stretch.** Leader-elected multi-instance scheduler using a lease (see 04 project 4.4).
**Resembles.** [Sidekiq](https://github.com/sidekiq/sidekiq) (retries and scheduled jobs), [Temporal](https://github.com/temporalio/temporal) (durable timers).

## 2.5 `logpipe`: a log pipeline with backpressure  `M`

**Goal.** Tail files -> parse (JSON and plaintext) -> enrich -> batch -> write to a rotating file sink (and an HTTP sink mock), with bounded channels end to end, at-least-once delivery, and a checkpoint of file offsets.
**Learn.** Tokio channels and backpressure, batching by size/time, checkpointing, rotation, graceful shutdown, cancellation safety.
**Read before.** `async/bounded_channel`, `async/worker_pool`, `async/graceful_shutdown`. **During.** `async/async_cancel_safety`, `dev_tools/tracing_logs`, `os_and_storage/file_disk_io_patterns`, `text_and_parsing/ndjson_serde`.
**Milestones.**
1. Single file tail with offset tracking; handles truncate and rename-rotation.
2. Parser stage: NDJSON, falling back to a `line` field for non-JSON; bad lines go to a dead-letter file.
3. Batcher: flush at 1,000 events or 500 ms, whichever first.
4. Sink with simulated slowness (sleep 50 ms per batch): upstream stalls rather than buffering unboundedly (assert RSS or queue length stays under 10 batches).
5. Checkpoint offsets after sink acks; kill and restart replays at most one batch.
**Acceptance.** Generate 2M lines, kill the process 5 times at random points: final sink contents contain every line at least once and duplicates are only within the last uncommitted batch; no more than 1 GiB RSS.
**Data.** Generator writing 2M lines (70% JSON, 25% plain, 5% garbage) with monotone sequence numbers to verify loss.
**Write-up.** Where is the single point where data can be lost? Why checkpoint after, not before, the sink ack?
**Stretch.** Fan-out to two sinks with independent backpressure; sampling and rate limits per source.
**Resembles.** [Vector](https://github.com/vectordotdev/vector), [Loki](https://github.com/grafana/loki) promtail.

## 2.6 `rsh`: a tiny Unix shell  `M`

**Goal.** A shell with pipelines (`a | b | c`), redirections (`>`, `>>`, `<`, `2>&1`), `cd`/`exit` builtins, background jobs (`&`), `fg`/`jobs`, and correct signal handling (Ctrl-C kills the foreground job, not the shell).
**Learn.** `fork/exec` model via `std::process`/`libc`, pipes and fd inheritance, process groups, `SIGINT`/`SIGCHLD`, zombie reaping, tokenization with quoting.
**Read before.** `os_and_storage/process_pipes_signals`, `os_and_storage/syscalls_portable`. **During.** `text_and_parsing/parser_combinators`, `os_and_storage/buffered_io_internals`.
**Milestones.**
1. Tokenizer handling `'single'`, `"double $VAR"`, escapes; 40 table-driven cases.
2. Run a single command with exit status in `$?`.
3. N-stage pipelines where every stage is spawned before waiting; `yes | head -1` terminates.
4. Redirections with correct ordering (`cmd > f 2>&1` vs `cmd 2>&1 > f`).
5. Job control: `sleep 100 &`, `jobs`, `fg %1`, Ctrl-Z if on a TTY.
**Acceptance.** A script-driven test (`rsh -c '...'`) compares stdout and `$?` against `sh -c` for 60 command lines; no zombie processes after 1,000 short commands (`ps` check); `yes | head -1` returns within 1 s.
**Data.** The 60-line corpus: pipes, redirects, quoting, empty pipeline stage (must be a syntax error), nonexistent command (exit 127).
**Write-up.** Why must the shell close pipe write-ends in the parent? What would deadlock if you did not?
**Stretch.** Globbing, `$(...)` command substitution, history with a line editor.
**Resembles.** [nushell](https://github.com/nushell/nushell), dash/bash.

## 2.7 `miniasync`: your own executor, timers and TCP echo  `L`

**Goal.** A single-threaded async runtime: `block_on`, `spawn`, `JoinHandle`, a timer wheel, `sleep`, and non-blocking TCP (via `mio` or raw kqueue/epoll) with an echo server benchmarked against tokio.
**Learn.** `Future`/`Waker`/`Pin`, task state machines, reactor + executor split, cancellation by drop, why `select!` needs cancel safety.
**Read before.** `async/futures_by_hand`, `async/waker_executor`, `networking/mio_event_loop`. **During.** `capstones/mini_async_runtime`, `networking/kqueue_epoll_raw`, `unsafe_ffi/pin_projection`, `os_and_storage/timing_wheel`.
**Milestones.**
1. `block_on` driving a hand-written `Future` (countdown of 3 polls).
2. `spawn` with a run queue and wakers that re-enqueue the task (`Arc<Task>` + `Wake`).
3. `sleep(d)` via timer wheel; 10,000 concurrent sleeps of random 1-100 ms all wake in order (never early).
4. Readiness-based `TcpListener::accept` and `TcpStream::read/write`.
5. Echo server with 1,000 concurrent connections; compare req/s to tokio on the same box.
6. `JoinHandle::abort` and a `timeout(fut, d)` combinator.
**Acceptance.** Executor polls a pending future zero times without a wake (counter test); spurious-wake test: waking 1,000 times polls at most once per ready state; 1,000 connections echo 1 KiB x 100 each with all bytes verified by checksum.
**Data.** `nc`/own client generating 1,000 connections; seeded delay distribution for timers.
**Write-up.** What exactly is the contract of `Waker::wake`? What does your executor do if a task wakes itself during `poll`?
**Stretch.** Multi-threaded work-stealing variant (reuse 2.3); `io_uring` backend on Linux.
**Resembles.** [tokio](https://github.com/tokio-rs/tokio), [mio](https://github.com/tokio-rs/mio).

## 2.8 `lfq`: lock-free queues verified with loom  `L`

**Goal.** Implement an SPSC ring buffer, a Treiber stack with safe reclamation (epoch or hazard pointers), and an MPSC queue, each with a loom model and a throughput benchmark.
**Learn.** Memory ordering (Acquire/Release), ABA, reclamation, cache-line padding, model checking with loom, Miri.
**Read before.** `concurrency/spsc_ring_buffer`, `concurrency/atomics_ordering`, `concurrency/memory_ordering_litmus`. **During.** `concurrency/lockfree_stack_aba`, `concurrency/epoch_hazard_pointer_reclamation`, `perf/false_sharing`, `concurrency/snapshot_cow_rcu`.
**Milestones.**
1. SPSC ring of capacity 2^k with cached head/tail indices; loom test of 2 producers-consumers interleavings.
2. Treiber stack with tagged pointers (ABA demo first: reproduce it with a deliberately buggy version).
3. Replace tagging with epoch-based reclamation; Miri shows no leaks or use-after-free.
4. MPSC (Vyukov intrusive) queue.
5. Benchmark 1M-10M msgs/s with and without padding; report ns/op.
**Acceptance.** loom explores all interleavings for 2-thread, 3-op scenarios without assertion failure; flipping one `Release` to `Relaxed` makes loom fail (mutation test, record which test caught it); total pushed == total popped across 4 stress runs of 10M ops.
**Data.** Sequence-numbered payloads so ordering and loss are checkable; a `Drop` counter for leak detection.
**Write-up.** For each atomic operation, which happens-before edge does it establish and who reads it?
**Stretch.** Bounded MPMC (Vyukov) ring; compare to `crossbeam` at 1,2,4,8 threads.
**Resembles.** [crossbeam](https://github.com/crossbeam-rs/crossbeam), [loom](https://github.com/tokio-rs/loom) test suites.

---

## Katas (30 minutes each)

1. Write a deadlock on purpose with two mutexes, detect it with a 2 s timeout test, then fix it with a global lock order.
2. Implement a `Semaphore` from `Mutex<usize>` + `Condvar`; test that 100 threads never exceed permit count 5 (atomic high-water mark).
3. Build a `CountdownLatch(n)`; 10 threads call `count_down`, one `wait()`; run 1,000 iterations.
4. Show a lost update with `load` + `store` on `AtomicU64`, then fix with `fetch_add`; 8 threads x 1M, final value exact.
5. Implement `OnceCell<T>` with `AtomicU8` states (empty/initializing/ready); 16 threads race and the init closure runs once.
6. Write a `Barrier` using `Condvar` and a generation counter; reuse it across 100 rounds with no thread running ahead.
7. Use `std::thread::scope` to compute word frequency in 8 chunks of a 50 MB generated text and match the sequential result.
8. Convert a blocking `mpsc` pipeline of 3 stages to `tokio::sync::mpsc` with capacity 8; show backpressure by printing queue depth.
9. Write `timeout(fut, d)` by hand using a `select`-like poll of two futures; test with a future that is ready after exactly 1 poll.
10. Make a `Mutex<Vec<T>>` poisoned on purpose and recover with `into_inner`; assert the data seen after recovery.
