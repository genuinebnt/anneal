> **Spoiler before you open the editor.** The starter holds reference code for `QueueState`, which decides when a task is queued.
> It also solves this preparation part. Solve it first:
>
> - **"Queue a job once, and lose no request"**
>
> You may replace the code with your own.

**Capstone, step 1 of 7.** A runtime holds thousands of futures and polls each one only when it can make progress. This step builds the unit it works with: a task, which is a future, plus the means to get itself polled again.

There is no executor yet. `Schedule` is the place tasks are sent to. In the tests it only collects them, and the test calls `run` by hand.

- `Task::spawn(future, scheduler)` makes a task and passes it to `scheduler.schedule` **once**. It does not poll. It returns another reference to the task.
- `run` is called by whoever took the task from the scheduler. It polls the future **once**, with a `Waker` that belongs to this task. Waking it must work from any thread, any number of times, for as long as anyone holds a clone.
- **A task is never waiting in the scheduler twice.** Wakes while it is already waiting for `run` do nothing.
- **No wake is lost.** If a wake arrives while the task is being polled and the poll returns `Pending`, `run` passes the task to `schedule` **once**, however many wakes arrived.
- `schedule` is **never called while the future is being polled**. It may call `run` straight away, on the same stack, and that must work.
- **A task is never polled by two threads at once.**
- When the future returns `Ready`, it is **dropped before `run` returns**, and `is_complete` becomes `true`. Wakes after that do nothing. The future's destructor may wake the task: nothing is scheduled then either.
- Whatever a thread wrote before it woke the task is visible to the poll that wake leads to.
- When nobody holds the task or one of its wakers any more, it is freed, together with a future that never finished and with its reference to the scheduler.

A panic inside `poll` is the next step's business. `run` is only ever called once for each time the task was scheduled.
