# Practice: async

**How to use this page.** Run each recipe (`cargo run --release --example <name>`), then answer "Prove you got it" with a written prediction before touching the code; async bugs hide behind "it seemed to work".
Do the [S] task right away, the [M] task within a week, and the [A] task as a standalone repo. Every task must end in an assertion, a timing bound with generous margin, or a provoked compile error.
Use `tokio::time::pause()` (feature `test-util`) for deterministic timing instead of real sleeps wherever you can. FlowForge async stages: attempt your stage first; this page only drills the underlying ideas in other domains.

## async_cancel_safety

**Prove you got it**
1. Predict how many values are lost when `read_pair` (recv, then recv) races a 10 ms timer inside `select!` in a loop with a producer sending every 7 ms. Count sent versus received, then fix it with caller-owned state and predict the new loss (zero).
2. Which of `mpsc::Receiver::recv`, `AsyncReadExt::read_exact`, `read` and `Mutex::lock` are cancel-safe? Write your prediction from the tokio `select!` docs, then prove two of them by experiment.

**Try next**
- [S] Add a test with 100k seeded random timer cuts that asserts the fixed reader returns every value exactly once and in order.
- [M] Make the unsafe version lose data on `read_exact` over a `tokio::io::duplex` pair (a frame split across two writes); then write a `FrameReader` that keeps a partial buffer in its own struct and passes the same test.
- [A] Write a line-oriented protocol client (handshake, then request/response with timeouts) in which every await point in a `select!` is cancel-safe; document each one in a table (point, safe/unsafe, why).

**Use it in**: any `select!` over sockets. Real code: tokio documents cancel safety per method; read the macro source in [tokio `select.rs`](https://github.com/tokio-rs/tokio/blob/master/tokio/src/macros/select.rs) and note how branches are polled in random order.

**Pitfall**: `tokio::time::timeout(dur, fut)` is `select!` in disguise; wrapping a non-cancel-safe future in it silently loses half-done work.

## async_patterns

**Prove you got it**
1. Predict total elapsed time for `join!` of three 100 ms sleeps versus three awaited in sequence. Then replace one sleep with `std::thread::sleep(100ms)` inside a `current_thread` runtime and predict what happens to the other two.
2. A spawned task panics: predict what `JoinHandle::await` returns and whether the runtime dies; then do the same inside `join!` (not spawned) and compare.

**Try next**
- [S] Wrap each of five tasks in `timeout`; assert the slow one reports `Elapsed` while the others return.
- [M] Mix CPU and IO: a pipeline where a CPU-heavy hash runs in `spawn_blocking` and results are collected via `JoinSet`; assert the async heartbeat task never stalls more than 50 ms (measure its tick jitter).
- [A] Build a fan-out/fan-in HTTP aggregator against three local mock servers (axum) with per-backend timeout, hedged second request after a delay, and a merged partial result; assert behaviour when one backend is dead, slow, or returns garbage.

**Use it in**: API gateways that call several backends. Real code: [axum](https://github.com/tokio-rs/axum) examples and [hyper](https://github.com/hyperium/hyper) clients; see [JoinSet's source](https://github.com/tokio-rs/tokio/blob/master/tokio/src/task/join_set.rs) for abort-on-drop behaviour.

**Pitfall**: dropping a `JoinSet` aborts its tasks; dropping a plain `JoinHandle` detaches them.

## async_streams_concurrency

**Prove you got it**
1. Predict the output order of `buffer_unordered(3)` versus `buffered(3)` for jobs with durations [300, 100, 200, 50, 50] ms, and the total time of each. Verify with the `Gauge` in-flight counter.
2. Predict what happens to concurrency if the consumer of the stream is slow (`for_each` that sleeps): do jobs keep starting? (Streams are lazy: reason it from the poll model, then check.)

**Try next**
- [S] Assert with the `Gauge` that max in-flight equals k exactly (not just at most k) for 100 jobs.
- [M] Add error handling: `try_buffer_unordered`-style early exit on the first error; assert the remaining futures are dropped (use a drop-tracking guard) and none starts afterwards.
- [A] Re-implement `buffer_unordered` yourself as a Stream combinator (poll the source, hold up to k futures in a `FuturesUnordered`-like structure) and run the same tests against yours and the library's.

**Use it in**: crawlers and bulk API callers. Real code: [futures-util `FuturesUnordered`](https://github.com/rust-lang/futures-rs/tree/master/futures-util/src/stream/futures_unordered).

**Pitfall**: `buffer_unordered` runs futures on the SAME task, so a CPU-heavy future stalls all of them; `tokio::spawn` each if you need parallelism.

## async_traits_dyn

**Prove you got it**
1. Predict the compile error for `Box<dyn Store>` when `Store` uses native `async fn`. Produce it and quote the key phrase.
2. Predict how many heap allocations one `DynStore::get` call makes versus the generic version. Check with the counting allocator from the perf chapter (`counting_allocator`).

**Try next**
- [S] Add a method with a default implementation to `Store` and see whether the blanket `DynStore` adapter still compiles.
- [M] Add a `Send` bound question: write a store whose future is not `Send` (holds an `Rc` across `.await`) and show `tokio::spawn` rejects it; then show `spawn_local` accepts it.
- [A] Build a pluggable storage layer (memory, file, flaky-failing fake) with a retry/timeout middleware wrapper that is itself a `Store`; test the middleware against the flaky fake with a seeded failure schedule.

**Use it in**: swappable backends with test fakes. Real code: [async-trait](https://github.com/dtolnay/async-trait) shows the boxing transformation that your adapter does by hand; compare also tower's `Service` in axum's stack.

**Pitfall**: boxing futures per call costs an allocation; for hot paths prefer generics.

## bounded_channel

**Prove you got it**
1. Predict the memory growth of an unbounded channel with a producer 10x faster than the consumer over 5 seconds, versus bounded(64). Measure RSS or queued count.
2. Predict `try_send` result on a full channel and on a closed channel; what does `send().await` return after the receiver is dropped?

**Try next**
- [S] Use `reserve().await` to get a permit, then fail before sending; assert the slot is released.
- [M] Add priority: two channels (high, low) consumed with `select!` and `biased;`; assert starvation behaviour of low under constant high load and add a fairness fix.
- [A] Build a pipeline of 4 stages with bounded channels and a metrics task reporting queue depth per stage once per 100 ms; identify and print the bottleneck stage automatically.

**Use it in**: stage hand-offs in ingest pipelines; real code: [tokio mpsc bounded channel module](https://github.com/tokio-rs/tokio/tree/master/tokio/src/sync) (semaphore-backed capacity).

**Pitfall**: bounded channels plus cycles (stage A sends to B and B sends back to A) deadlock when both are full.

## futures_by_hand

**Prove you got it**
1. Predict how many times `CountDown::poll` runs for a count of 3 under `block_on`, and what happens if `poll` returns `Pending` without registering the waker (does your executor hang? why or why not in your version?).
2. Predict what `pipeline` does if you never poll it, and what drop of a half-polled future does to its locals.

**Try next**
- [S] Write `Yield` (returns Pending once, wakes itself) and assert exactly 2 polls.
- [M] Write `Join2<A,B>` combinator by hand; assert both futures are polled and that the result is ready only when both are, using a poll-count instrument.
- [A] Hand-write the state machine that the compiler would generate for `async fn pipeline` (an enum with one variant per await point) and assert same output; note what `Pin` guarantees for the self-referential version.

**Use it in**: debugging "task never completes". Real code: [futures-rs](https://github.com/rust-lang/futures-rs) combinators are exactly this style; [async-task](https://github.com/smol-rs/async-task) shows a minimal task cell.

**Pitfall**: returning `Pending` without arranging a wake-up hangs the task forever.

## graceful_shutdown

**Prove you got it**
1. Predict what happens to 10 queued jobs when the shutdown signal arrives while job 3 runs. Count processed jobs; then remove the drain loop and predict the count.
2. Predict the difference between `watch` and `Notify` for this signal when a worker starts AFTER the signal was sent.

**Try next**
- [S] Add a hard deadline: if draining takes longer than 2 s, abandon the rest and report how many were dropped.
- [M] Run N workers sharing one receiver (`Arc<Mutex<Receiver>>` or one channel each) and assert all queued jobs are processed exactly once at shutdown.
- [A] Wire OS signals: handle SIGTERM and SIGINT with `tokio::signal`, run a worker pool, send the signal from a child-process test harness (`kill -TERM` through `Command`), and assert exit code 0 with all jobs finished.

**Use it in**: Kubernetes pods draining on SIGTERM. Real code: [tokio signal module](https://github.com/tokio-rs/tokio/tree/master/tokio/src/signal) and the [`watch` channel](https://github.com/tokio-rs/tokio/blob/master/tokio/src/sync/watch.rs).

**Pitfall**: a worker blocked inside a long `await` that is not in the `select!` never sees the signal.

## select_cancel

**Prove you got it**
1. Predict the winner and loser output for `select!` between a 50 ms and a 100 ms sleep, and whether the loser's destructor runs. Add a guard that prints on drop.
2. Predict the effect of `biased;` when both branches are ready, over 1000 iterations (counts per branch).

**Try next**
- [S] Add a cancellation token branch and a test that cancelling during a multi-step operation drops the in-flight future (drop guard fires once).
- [M] Race three mirrors; the first success wins, failures are ignored unless all fail; assert the error returned lists all three failures.
- [A] Write a cache-aside lookup that races cache against DB with a 20 ms hedge, then repopulates the cache even if the caller was cancelled (hint: spawn the repopulation); test with `time::pause()`.

**Use it in**: request timeouts and hedged requests. Real code: [tokio `select!`](https://github.com/tokio-rs/tokio/blob/master/tokio/src/macros/select.rs).

**Pitfall**: side effects performed before the first `.await` inside a losing branch have already happened.

## waker_executor

**Prove you got it**
1. Predict how many times a task is polled when a `Timer` for 100 ms is awaited, and what happens if the timer thread forgets to call `wake`. Instrument poll count and verify.
2. Predict what happens if a task wakes itself 1000 times before returning Ready: does it starve other tasks? Test with a second task that should make progress.

**Try next**
- [S] Add a `yield_now` future and a test that two cooperating tasks interleave their output.
- [M] Add `JoinHandle<T>` to your executor (result passed via shared slot plus waker); assert it works for 1000 tasks and for a panicking task (catch with `catch_unwind`).
- [A] Add a multi-threaded mode: N worker threads pull from a shared run queue; handle the "wake while being polled" race (a task state machine IDLE/SCHEDULED/RUNNING/NOTIFIED); stress with 100k tasks and assert each is polled to completion exactly once.

**Use it in**: test harnesses and embedded runtimes. Real code: [async-task](https://github.com/smol-rs/async-task) and [tokio's task module](https://github.com/tokio-rs/tokio/tree/master/tokio/src/runtime/task).

**Pitfall**: lost wakeups: the notification arrives between "poll returned Pending" and "task parked"; a state machine with a NOTIFIED bit is the standard cure.

## worker_pool

**Prove you got it**
1. Predict the `Outcome` counts for 200 jobs where every 10th panics, with pool size 4. Then predict how `finish().await` behaves if a job never completes.
2. Measure it: jobs/s at pool sizes 1, 2, 4, 8, 16 for a job that sleeps 5 ms (IO-like) versus one that hashes for 5 ms of CPU. Predict the plateau for each; the CPU-bound one flattening at your core count is the point, and a CPU-bound job on the async pool that does not flatten means you are not actually blocking.

**Try next**
- [S] Add per-job timeout producing `Outcome::TimedOut`; assert the permit is released.
- [M] Add retries with exponential backoff and jitter (compose with `distributed/retry_backoff_jitter`); assert retry count never exceeds the budget and total elapsed is bounded.
- [A] Turn the pool into a priority pool with fair-share between tenants (deficit round robin; see `os_and_storage/scheduler_policies`), and show with a test that a tenant flooding the queue cannot starve another.

**Use it in**: image or video processing farms. Real code: [mini-redis](https://github.com/tokio-rs/mini-redis) for a small tokio server and the [tokio runtime schedulers](https://github.com/tokio-rs/tokio/tree/master/tokio/src/runtime/scheduler) for how a production run queue is built.

**Pitfall**: panic isolation requires a `tokio::spawn` per job; `catch_unwind` around an async block is easy to get wrong.

## Chapter projects

### Small (about 2 h): resilient URL checker
Combine `async_streams_concurrency`, `select_cancel`, `async_cancel_safety`, and `errors/error_chain_retryable`.
- M1: read a list of URLs (use local test servers), check with `buffer_unordered(k)`.
- M2: per-URL timeout, retry on retryable errors only.
- M3: summary report sorted deterministically.
- Accept: with a mock server that is slow, flaky, or dead, report counts match the seeded schedule; max in-flight never exceeds k.
- Stretch: global deadline that cancels the whole run and prints partial results.

### Intermediate (a weekend): chat/pub-sub server with graceful shutdown
Combine `bounded_channel`, `graceful_shutdown`, `actor_model` (concurrency), `networking/tcp_framed_server`, and `networking/websocket_frames`.
- M1: framed TCP server; each connection is a task feeding a bounded outbound queue.
- M2: a room actor owning the subscriber list; slow subscribers are disconnected when their queue is full.
- M3: SIGTERM drains and notifies clients.
- Accept: 200 clients, 10k messages seeded; every client receives each message in its subscribed rooms in order or is disconnected with a reason; no task leak (TaskTracker reports zero at exit).
- Stretch: speak WebSocket framing for browser clients.

### Advanced (a week): your own mini async runtime plus an echo benchmark
Combine `waker_executor`, `futures_by_hand`, `thread_pool_work_stealing` (concurrency), `networking/kqueue_epoll_raw`, `os_and_storage/timing_wheel`.
- M1: single-thread executor with `spawn`, `JoinHandle`, `yield_now`.
- M2: reactor on the raw poller with `TcpListener`/`TcpStream` futures (readiness to wakers).
- M3: timers from the timing wheel; `sleep` and `timeout`.
- M4: multi-thread work-stealing mode.
- Accept: echo server handles 10k idle connections with under 1 percent CPU, 1000 active connections echo 100 MB with checksum match; timers fire within 2 ms of deadline in a seeded test; compare throughput with tokio on the same benchmark and write up the gap.
- Stretch: cancellation-safe `AsyncRead` adapter and a `select!` macro.
