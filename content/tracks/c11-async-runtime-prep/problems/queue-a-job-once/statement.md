A thumbnail service redraws a folder whenever a file in it changes. Changes come from many threads, often in bursts. Redraw jobs go into a queue that worker threads serve.

Two things must not happen. A folder must not sit in the queue twice, or two workers redraw it at once. And a change that arrives **while** a redraw runs must not be lost, because that redraw may already have read the old files.

Write `QueueState`, the small shared value that makes both promises for **one** job. It does not hold a queue. It only tells callers what to do. Use one atomic, no lock.

A new job is **idle**.

- `request()`: "this job has new work". It returns `true` when the caller must **now put the job in the queue**, and `false` otherwise. After a `true`, every further request returns `false` until the job has run.
- `begin()`: a worker took the job from the queue and starts running it. It **panics** if the job is not queued.
- `end()`: the run is over. It returns `true` if the job must **go back in the queue**, because at least one `request()` arrived since `begin()`.
  - The job then counts as queued again. Several such requests give one `true`.
  - Otherwise it returns `false`, and the job is idle.
  - It **panics** if the job is not running.
- `finish()`: the run is over and the job is done **for ever**, even if requests arrived during it. Afterwards `request()` returns `false` and `begin()` panics. `finish()` **panics** if the job is not running.
- A call that panics changes nothing.

**Threads.** `request` may be called from any thread at any moment. `begin`, `end` and `finish` are only ever called by whoever holds the queued or running job.

**What a run can see.** Whatever a thread wrote before it called `request()` is visible to a run whose `begin()` comes after that request. Think about every request, including one that returns `false`. The tests can't force this to fail on every machine, so check it by reasoning about orderings.
