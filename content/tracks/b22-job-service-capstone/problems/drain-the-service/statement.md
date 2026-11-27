> **Spoiler before you open the editor.** The starter of this step contains reference code that solves these earlier steps:
>
> - Refuse a bad job and say why
> - Keep the jobs and accept each once
> - Post a job, read it and page through jobs
> - Run the jobs, a few at a time
> - Cancel a job
> - Watch a job as it happens
>
> It also gives away much of these preparation problems. Solve them first:
>
> - One error shape for the whole API
> - Charge the card once
> - Pages that hold still while the list changes
> - Background jobs within a memory budget
> - Run one piece of work and say how it ended
> - A progress feed you can leave and rejoin
> - Live prices in the browser
>
> The code is a stand-in, because the app cannot yet carry your own code from one step to the next. You may replace it with your own; keep the public names.

**Capstone, step 7 of 8.** The service is being redeployed. It must stop accepting jobs, finish the ones it has promised, and report what happened.

**`JobService::shutdown().await`** closes the service to new jobs as soon as it is first polled. Then it waits until every accepted job is final. It returns a `ShutdownReport`: the numbers of jobs in `total`, `succeeded`, `failed` and `cancelled`, and `unfinished`, which is 0. With nothing unfinished, it returns at once.

**While closing, and ever after:**

- a **new** key is refused with `ApiError::ShuttingDown`, by `submit` and so by `POST` (503). No job is created and no work is called;
- a **retry** of a known key still returns its job (200). A known key with another spec is still `KeyReuse`;
- a request that is invalid on its own, such as one without a key or with a bad body, keeps its usual error;
- reading, listing, cancelling and following all still work. Cancelling lets the shutdown finish sooner;
- queued jobs still run.

**The guarantee.** This holds with submits racing the shutdown on many threads. Every job that `submit` ever accepted is final when `shutdown` returns, and is counted in its report.

**For the tests.** `JobStore::on_admit(hook)` stores a hook; only the first one given is kept. `submit` calls it once for each **new** job it is about to create: right after it has found the service open, as the very next thing, before anything is created. A retry, a reused key and a refused key do not call it. The tests use it to pause one `submit` and poll `shutdown` from another thread. The hook may take a long time, and it never calls the service on its own thread.

The check and the change must still be one step. The tests only probe the gap right after the hook.

**Waiting.**

- Any number of callers may call `shutdown`, together or later. Each returns only when all jobs are final.
- Dropping a `shutdown` future, for example when a time limit expires, **stops no job**. The service stays closed.
- When `shutdown` returns, no work future is left alive.
- It has no deadline of its own: a caller adds one around it.

All earlier rules still hold.
