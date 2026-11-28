Two ways to sum in parallel. Both take `sum`, the function that sums one piece: call it on every piece,
each piece on its own thread.

- `sum_chunks` owns its chunks: sum each on a `thread::spawn`ed thread and return the totals in chunk
  order.
- `sum_parts` only borrows `data`, so `thread::spawn` can't use it. Split it into `parts` contiguous
  pieces whose lengths differ by at most one, longer pieces first (10 items in 3 parts: 4, 3, 3), and
  sum each piece on its own **scoped** thread. Return the sums in order. `parts` is at least 1 and may be
  larger than `data.len()`, which gives empty pieces.

Start every thread before joining any of them. Some tests pass a `sum` that waits until every piece is
being summed at the same time. Threads started one after another never all meet, so after 3 seconds
that test gives up and fails.
