> **Spoiler before you open the editor.** The starter of this step contains reference code that solves these earlier steps:
>
> - Refuse a bad job and say why
> - Keep the jobs and accept each once
> - Post a job, read it and page through jobs
>
> It also gives away much of these preparation problems. Solve them first:
>
> - One error shape for the whole API
> - Charge the card once
> - Pages that hold still while the list changes
>
> The code is a stand-in, because the app cannot yet carry your own code from one step to the next. You may replace it with your own; keep the public names.

**Capstone, step 4 of 8.** Now do the work. A `JobService` is a `JobStore` plus workers. Clones are handles to the same service.

`JobService::new(max_running, work)` takes the work as a function: `work(spec)` gives a future that ends in `Ok(output)` or `Err(message)`. The tests pass their own. `max_running == 0` panics.

- `submit(key, spec)` answers like the store's. For a **new** job it also arranges for the work to run. It returns at once, without calling `work`. It is only called inside a Tokio runtime.
- `work` is called **exactly once** for each new job, and never for a retry or a refused call. It receives the job's whole `Spec`.
- At most `max_running` jobs are running at once. Jobs take free slots **in id order**, as soon as one is free. (The tests of order use a current-thread runtime.)
- A job is `Running` exactly while it holds a slot. It is already `Running` when `work` is called. While it waits it is `Queued`.
- `Ok(output)` makes the job `Succeeded` with that output. `Err(message)` makes it `Failed` with that error.
- A **panic** makes the job `Failed` with the error `worker panicked: <message>`. This covers a panic in the future and one in the call to `work` itself. The message is the payload if it is a `String` or `&str`, else `unknown panic`.
- However a job ends, its slot is free again, and other jobs are not affected.
- `store()` gives the service's store.
- `held_tasks()` is the number of worker task handles the service keeps. Finished ones are let go no later than the next new job. So after finished workers have had time to end, submitting a new job leaves it equal to the number of unfinished jobs.

**HTTP.** `router` now takes a `JobService` and behaves as in step 3, with `POST` submitting through the service. The starter still has the old handlers, which use a `JobStore`. Change them.
