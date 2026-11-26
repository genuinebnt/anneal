# Building: A high-performance async network server



Use this when you have a real project in front of you. For each stage: **attempt your design first**, then read the listed recipes to compare, steal what is better and note what you missed.

### Step 1: Foundations

**Goal:** Futures.

| # | Recipe | Why here |
|---|---|---|
| 1.1 | [`futures_by_hand`](../../examples/async/futures_by_hand.rs) · advanced | Poll. |
| 1.2 | [`waker_executor`](../../examples/async/waker_executor.rs) · advanced | Executor. |
| 1.3 | [`async_cancel_safety`](../../examples/async/async_cancel_safety.rs) · advanced | Cancellation. |

### Step 2: Server

**Goal:** Many connections.

| # | Recipe | Why here |
|---|---|---|
| 2.1 | [`mini_async_runtime`](../../examples/capstones/mini_async_runtime.rs) · capstone | Runtime. |
| 2.2 | [`mini_http_server_thread_pool`](../../examples/capstones/mini_http_server_thread_pool.rs) · capstone | Threaded baseline. |
| 2.3 | [`http1_parser`](../../examples/networking/http1_parser.rs) · advanced | Protocol. |
| 2.4 | [`async_streams_concurrency`](../../examples/async/async_streams_concurrency.rs) · intermediate | Streams. |
| 2.5 | [`graceful_shutdown`](../../examples/async/graceful_shutdown.rs) · intermediate | Shutdown. |
| 2.6 | [`tower_service_layer`](../../examples/patterns/tower_service_layer.rs) · advanced | Middleware. |

### Step 3: Tuning

**Goal:** Measure and fix.

| # | Recipe | Why here |
|---|---|---|
| 3.1 | [`kqueue_epoll_raw`](../../examples/networking/kqueue_epoll_raw.rs) · advanced | Event loops. |
| 3.2 | [`io_uring_model`](../../examples/os_and_storage/io_uring_model.rs) · advanced | Next-gen I/O. |
| 3.3 | [`false_sharing`](../../examples/perf/false_sharing.rs) · advanced | Contention. |
| 3.4 | [`bench_harness`](../../examples/perf/bench_harness.rs) · intermediate | Benchmark. |
