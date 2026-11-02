A server records the latency of each request from many threads. Write `Stats`: `record(&self, v)` adds one
observation, `snapshot(&self)` returns `Some(Snapshot { count, sum, min, max })` or `None` when nothing was
recorded, and `take(&self)` returns the same and resets to empty (called only when no `record` is running).

`record` takes `&self`, so `Stats` is shared by reference (or `Arc`) with no lock. The `sum` must **saturate**
at `u64::MAX` instead of wrapping, and no update may be lost however many threads record at once.
