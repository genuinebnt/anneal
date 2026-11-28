> **Spoiler before you open the editor.** The starter holds reference code for **capstone steps 1 to 5**: tasks, join handles, the current handle, timers and work queues.
> It also solves these preparation parts. Solve them first:
>
> - **"Queue a job once, and lose no request"**
> - **"Say how a future ended, after it has cleaned up"**
> - **"The current one, on this thread"**
> - **"Go to sleep without missing work"**
>
> You may replace the code with your own; keep the public names.

**Capstone, step 6 of 7.** Put the pieces together: a runtime with several worker threads, a thread for timers, and a shutdown that leaves nothing behind. `LocalRuntime` and the manual clock are gone. Time is now real time since the runtime started. The crate `crossbeam` is available.

**Running.**

- `Runtime::new(n)` starts `n` worker threads and one timer thread. `n == 0` panics.
- `spawn`, through the `Runtime` or a `Handle`, from any thread, runs the task on a **worker** thread. It never runs on the caller.
- `block_on(future)` drives `future` on the **calling** thread; the workers run the tasks meanwhile. The future need not be `Send` or `'static`. The thread sleeps while the future is pending, and any thread may wake it.
- While `block_on` runs, and on every worker, this runtime's handle is **current**. So `spawn` and `sleep` work inside the root and inside tasks.
- **No worker sleeps while a task is waiting to run.** In particular, `n` tasks can block their `n` threads at the same moment, wherever they were spawned from.
- A lost wake-up is a bug, not a delay: **don't put a timeout on a sleeping worker** to cover for it. Idle workers must sleep, not spin. (The tests can't measure the spinning.)
- **Searching for work must not block other workers.** While one worker is inside `next_task`, the others must be able to search, find nothing and go to sleep. So don't put a lock around it. The batch-move test pauses one worker in the middle of its search and waits for another to go to sleep.
- A task that panics does not harm its worker.

**Keep the given test points.** One test forces the schedule behind the rule above: a worker looks for work while another is moving a batch between queues. It needs three things from the given code, even if you change the rest. `next_task` takes from the shared queue with `take_injected`, which moves batches. Idle workers sleep in the given `Gate`. The module `probe`, its two calls in `take_injected` and its one call in `Gate::sleep` stay.

**Time.** `sleep`, `timeout` and `interval` work as in step 4, on the real clock. A sleep is **never early**. It must fire although the timer thread was already waiting for a **later** one.

**Dropping the runtime.** When `drop` returns:

- every worker thread and the timer thread have **exited**. Drop waits for polls that are in progress, but **not** for queued tasks: a task that yields for ever must not hold it up;
- **every task** spawned on this runtime that had not finished has had its future **destroyed**, and its handle resolves to `Cancelled`. This includes sleeping tasks and tasks waiting for a waker that someone else still holds;
- while those futures are destroyed, this runtime's handle is current. Their destructors may spawn;
- spawning through a `Handle` after that (or during the drop) destroys the future without polling it; the handle resolves to `Cancelled`;
- the runtime holds no more references to anything the tasks owned.

A runtime is never dropped from one of its own tasks, and `block_on` is never called from inside a task.
