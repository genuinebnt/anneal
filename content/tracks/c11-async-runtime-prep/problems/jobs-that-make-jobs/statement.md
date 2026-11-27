A build system starts with a few targets. Compiling one reveals more: its dependencies, then theirs. A crawler works the same way. You don't know the full list of jobs in advance, and several threads must work through it.

Write `run(workers, jobs, handle)`. It starts `workers` threads and calls `handle(job, &spawner)` for every job. A handler may add jobs with `spawner.spawn(job)`; `spawner.worker()` is the number (from 0) of the worker running it. The crate `crossbeam` is available.

- Every job, given or spawned, is handled **exactly once**.
- The given `jobs` are started in the order given. With several workers they may then run at the same time.
- `run` returns when **all** jobs are done, including everything spawned along the way. All its threads have exited by then.
- `Report::per_worker[i]` counts the jobs worker `i` handled. Its length is `workers`.
- **No worker waits while a job is waiting.** If a worker is free and a job has not started, a free worker takes it. This holds even when the job was spawned by a worker that is now busy for a long time. So `workers` jobs can all be running at once.
- **Own work first.** A worker does not start one of the given `jobs` while a job that **it** spawned has not started (unless another worker takes that one). With one worker, everything a given job spawns, directly or not, runs before the next given job.
- Spawning must be cheap enough for hundreds of thousands of jobs, and a long chain of jobs spawning jobs must not grow the stack.
- If a handler **panics**, `run` panics too. It does not hang. The other jobs may or may not be handled, but every job given to `run` or spawned is **dropped** before the panic leaves `run`.
- No jobs: `run` returns at once. `workers == 0` panics.

A worker with nothing to do may spin or yield here. Letting it sleep is the next part.
