A service imports uploaded files in the background. A small file needs a little memory while it is processed, and a huge one needs a lot. If too many run at once, the process is killed. The upload request must not wait for any of this.

Write an `Importer` that runs jobs in the background within a fixed **budget**. Each job is a future with a cost.

- `Importer::new(budget)` panics if the budget is 0.
- `submit(cost, work)` returns at once; it is not `async`. It is only called inside a Tokio runtime. A cost of 0, or one larger than the whole budget, gives `Err(BadCost)` and `work` is never polled. Otherwise the job is accepted and runs on its own task.
- A job **starts** (its future is first polled) only when its cost fits. The costs of started, unfinished jobs never add up to more than the budget.
- Jobs start **in the order they were submitted**. A small job never starts before an earlier large one, even if the small one would fit now. (The tests check order on a current-thread runtime.)
- A job starts as soon as these two rules allow. Jobs that fit together run at the same time.
- A job's cost is returned however it ends, a **panic** included. A panic affects no other job.

**Waiting.** `idle().await` returns when no accepted job is unfinished, queued ones included. It returns at once when there is none. Several callers may wait together. Dropping an `idle()` future, for example because a timeout expired, has **no effect on any job**. `submit` may be called while somebody is waiting.

**Memory.** The service runs for months, so it must not keep finished jobs. `held()` is the number of task handles the importer is keeping. Finished ones are let go no later than the next accepted `submit`. So once finished tasks have had time to end, the next `submit` leaves `held()` equal to the number of unfinished jobs, the new one included.

**Dropping the `Importer`** cancels every unfinished job: their futures are dropped soon after, and queued ones never start.

All methods take `&self`.
