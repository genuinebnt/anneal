> **Spoiler before you open the editor.** The starter holds reference code for **capstone steps 1 and 2** (tasks and join handles).
> It also solves these preparation parts. Solve them first:
>
> - **"Queue a job once, and lose no request"**
> - **"Say how a future ended, after it has cleaned up"**
>
> You may replace the code with your own; keep the public names.

**Capstone, step 5 of 7.** A runtime with several worker threads needs queues that threads don't fight over. Each worker has **its own** queue. One **shared** queue takes work from outside.

This step builds the queues and the rule for what a worker takes next. No threads are started here: the tests drive workers by hand. The crate `crossbeam` is available.

`pool(n)` returns the shared `Pool`, which is a `Schedule`, and `n` `LocalWorker`s numbered from 0. `n == 0` panics.

**Entering.** `worker.enter()` returns an `EnteredWorker`: while it lives, this thread is that worker. A `LocalWorker` is `Send`; its queue and its count survive between enters and move with it. One thread may enter several workers, nested. The guards may be dropped in any order; the innermost live one is the innermost.

**Scheduling.** If the **innermost** entered worker on the calling thread belongs to **this** pool, the task goes to that worker's own queue. In every other case it goes to the shared queue.

**`next_task()`** takes one task for its worker.

- Every call counts, including calls that return `None`. The count belongs to the worker and goes on across enters.
- Normally it looks in this order: the worker's **own** queue, then the **shared** queue, then **other workers'** queues.
- On every **61st** call (the 61st, 122nd, ...) it looks at the **shared** queue first, and goes on as usual if that is empty.
- Own queue: **first in, first out**. Tasks a worker schedules for itself come out in the order scheduled, unless another worker takes them.
- Shared queue: the **oldest** task comes out first.
- When taking from the shared queue or from another worker, it may move several tasks into its own queue at once. It does not have to.
- It returns `None` only if every queue is empty. While other threads are working on the queues, a momentary failure to take a task is not "empty".

**Also:**

- `has_local_tasks()` says whether the entered worker's own queue holds tasks.
- `has_work()` says whether any queue, shared or own, holds a task.
- `steals(i)` is the number of `next_task` calls by worker `i` whose task came out of **another worker's** queue.
- Every scheduled task is returned **exactly once**, however many threads are involved. `Pool` is `Send + Sync`.
