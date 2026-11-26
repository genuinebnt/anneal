# Async

Futures by hand, executors and wakers, streams, cancellation safety, async traits and a mini runtime.

Run any recipe from the `cookbook/` directory: `cargo run --release --example <name>` (the name is the file name without `.rs`). Each file starts with a header: what it shows, `LEVEL`, `COMMON USES` and `Read:` links.

10 examples.

| Example | What it shows | Level | Common uses | Crates |
|---|---|---|---|---|
| [`select_cancel`](select_cancel.rs) | cancellation with `select!`, and timeouts | intermediate | request timeouts, shutdown signals, racing a cache against a database: select!/timeout drop the loser. | tokio |
| [`bounded_channel`](bounded_channel.rs) | bounded channels and backpressure | intermediate | producer/consumer queues where memory must stay bounded: backpressure is the whole point. | tokio |
| [`graceful_shutdown`](graceful_shutdown.rs) | graceful shutdown: stop accepting work, drain what is queued, then exit | intermediate | servers and workers: finish in-flight work on SIGTERM instead of dropping jobs (Kubernetes rolling deploys). | tokio |
| [`futures_by_hand`](futures_by_hand.rs) | what a Future really is: poll it by hand with a 10-line executor | advanced | debugging stuck async tasks, writing executors/combinators, understanding cancellation. | std only |
| [`async_patterns`](async_patterns.rs) | join!, spawn, JoinSet, spawn_blocking, timeouts, and panic isolation in tokio | intermediate | fan-out API calls, mixing CPU and IO work, per-task timeouts and panic isolation in services. | tokio |
| [`waker_executor`](waker_executor.rs) | build a real executor: tasks, wakers, a ready-queue, and a timer future that wakes from another thread | advanced | understanding tokio internals; custom executors for embedded/real-time/test harnesses. | std only |
| [`async_streams_concurrency`](async_streams_concurrency.rs) | bounded concurrency with Streams: `buffer_unordered`, `buffered`, `FuturesUnordered`, `for_each_concurrent` | intermediate | crawlers, batch API calls, ETL: at most K concurrent futures with streaming results. | futures-util, tokio |
| [`async_traits_dyn`](async_traits_dyn.rs) | `async fn` in traits, why `dyn` needs boxing, and an adapter that makes any impl object-safe | advanced | swappable async backends: storage/queue/clock traits with fakes in tests and dyn dispatch at runtime. | tokio |
| [`async_cancel_safety`](async_cancel_safety.rs) | cancellation safety: why `select!` can silently lose data, and the fix | advanced | every select!/timeout loop: do not lose half-read frames or half-built batches. | tokio |
| [`worker_pool`](worker_pool.rs) | a generic async worker pool: bounded queue, bounded concurrency, panic isolation, graceful drain | advanced | image/video processing farms, scraping pools, job runners: bounded queue + bounded workers + panic isolation. | tokio |
