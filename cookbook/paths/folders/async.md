# Chapter `async`: suggested reading order

Ordered simple -> intermediate -> advanced (then alphabetical). Skim `Use in:` and `COMMON USES` in each header first. Cross-chapter order is in [the tracks](../tracks/).

| # | Recipe | Level | What it is |
|---|---|---|---|
| 1 | [`async_patterns`](../../examples/async/async_patterns.rs) | intermediate | join!, spawn, JoinSet, spawn_blocking, timeouts, and panic isolation in tokio. |
| 2 | [`async_streams_concurrency`](../../examples/async/async_streams_concurrency.rs) | intermediate | bounded concurrency with Streams: `buffer_unordered`, `buffered`, `FuturesUnordered`, `for_each_concurrent`. |
| 3 | [`bounded_channel`](../../examples/async/bounded_channel.rs) | intermediate | bounded channels and backpressure. |
| 4 | [`graceful_shutdown`](../../examples/async/graceful_shutdown.rs) | intermediate | graceful shutdown: stop accepting work, drain what is queued, then exit. |
| 5 | [`select_cancel`](../../examples/async/select_cancel.rs) | intermediate | cancellation with `select!`, and timeouts. |
| 6 | [`async_cancel_safety`](../../examples/async/async_cancel_safety.rs) | advanced | cancellation safety: why `select!` can silently lose data, and the fix. |
| 7 | [`async_traits_dyn`](../../examples/async/async_traits_dyn.rs) | advanced | `async fn` in traits, why `dyn` needs boxing, and an adapter that makes any impl object-safe. |
| 8 | [`futures_by_hand`](../../examples/async/futures_by_hand.rs) | advanced | what a Future really is: poll it by hand with a 10-line executor. |
| 9 | [`waker_executor`](../../examples/async/waker_executor.rs) | advanced | build a real executor: tasks, wakers, a ready-queue, and a timer future that wakes from another thread. |
| 10 | [`worker_pool`](../../examples/async/worker_pool.rs) | advanced | a generic async worker pool: bounded queue, bounded concurrency, panic isolation, graceful drain. |
