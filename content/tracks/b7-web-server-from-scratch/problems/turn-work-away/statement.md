A service has a fixed number of workers. When work arrives faster than they finish it, an unlimited backlog only makes every answer later and uses more memory. This service says no instead.

`Service::start(workers, room, handle)` starts `workers` threads that run `handle(item)` for the items offered to it. Both numbers are at least 1; panic otherwise.

- `offer(item)` never blocks. It accepts the item unless `room` accepted items are already waiting for a worker, in which case it returns `Err(item)`. A refused item is never handled.
- Items being handled do not count as waiting. Up to `workers` items are handled at the same time, and the service never starts more than `workers` threads.
- `waiting()` is the number of accepted items no worker has taken yet. While offers and workers are running it may lag, but it is never larger than `room` plus the number of workers plus the number of `offer` calls in progress.
- A `handle` that panics costs nothing but that item: the number of working workers stays the same.
- `close()` handles every item already accepted, waits for the workers to end, and returns the totals. Dropping a `Service` without calling it does the same. After either, the service holds nothing of `handle`.

`Totals { accepted, refused, handled, panicked }`: `accepted` and `refused` count offers; `handled` counts items whose `handle` returned, `panicked` those whose `handle` panicked.
