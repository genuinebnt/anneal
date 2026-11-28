> **Spoiler before you open the editor.** The starter holds reference code for **capstone steps 1 and 2** (tasks and join handles).
> It also solves these preparation parts. Solve them first:
>
> - **"Queue a job once, and lose no request"**
> - **"Say how a future ended, after it has cleaned up"**
>
> You may replace the code with your own; keep the public names.

**Capstone, step 3 of 7.** Now something has to run the tasks. `LocalRuntime` does it on the caller's own thread.

**The current runtime.** `Handle` spawns tasks onto a runtime and can be cloned and sent to other threads.

- `handle.enter()` makes it the thread's **current** handle until the `Entered` guard is dropped. Guards nest; dropping them in any order leaves the innermost live one current.
- `Handle::current()` returns it, and panics outside a runtime. `try_current()` returns `None` there.
- The free function `spawn` spawns on the current handle, and panics outside a runtime.
- Each thread has its own current handle.

**The runtime.** Tasks are run in the order they were scheduled (first in, first out). Spawning only queues.

- `run_until_stalled()` runs queued tasks until the queue is empty, including tasks queued while it runs. It returns the number of runs. It never sleeps.
- `block_on(future)` drives `future` on this thread. The future need not be `Send` or `'static`. It is polled **first**, before any task runs, and `block_on` returns **as soon as** it is ready. Tasks that are left stay in the runtime for later.
- **After each poll of the root that returns `Pending`, if a task is queued, at least one runs** before the root is polled again. So a root that keeps waking itself starves nobody.
- **Once the root is woken, at most one more task runs** before it is polled. So tasks that keep re-queuing themselves can't starve the root.
- The root is polled again only after it was woken (and once at the start).
- With nothing to do, `block_on` **sleeps**. A wake of the root, a wake of a task, or a spawn through a `Handle`, from **any thread**, wakes it.
- While `block_on` or `run_until_stalled` runs, the runtime's handle is current, for the root and for every task. Afterwards the previous one is current again, also if the root panics.

`yield_now().await` is pending exactly once, waking its own waker first, so the task goes to the back of the queue.

**Dropping the runtime.**

- Every **queued** task has been destroyed when drop returns, so its handle resolves to `Cancelled`. This holds even if someone else still holds a waker for it, so dropping the queue's reference is not always enough.
- While drop destroys them, the runtime's handle is **current**. Destructors that run then may call `spawn`; the new task is destroyed too, before drop returns.
- After that, `handle.spawn` through an old handle destroys the future without polling it. Its `JoinHandle` resolves to `Cancelled`.
- Tasks that are waiting for a wake from somewhere else are not covered here; step 6 deals with them.

The tests never call `block_on` from inside a task of the same runtime.
