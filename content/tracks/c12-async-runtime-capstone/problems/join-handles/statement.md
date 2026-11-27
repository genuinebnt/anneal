> **Spoiler before you open the editor.** The starter holds reference code for **capstone step 1** (`Task`).
> It also solves these preparation parts. Solve them first:
>
> - **"Queue a job once, and lose no request"**
> - **"Say how a future ended, after it has cleaned up"**
>
> You may replace the code with your own; keep the public names.

**Capstone, step 2 of 7.** `Task::spawn` runs a future but gives nothing back. Whoever spawns work usually wants its result, wants to hear that it crashed, and sometimes wants to stop it.

`spawn_on(&scheduler, future)` spawns `future` as a task, scheduled once and not polled, and returns a `JoinHandle<T>`.

**The result.** Awaiting the handle gives:

- `Ok(output)` when the future returned it;
- `Err(JoinError::Panicked(message))` when its poll or its destructor panicked. The message is the panic's text (`&str` or `String`), else `"non-string panic"`. **`run` does not panic.**
- `Err(JoinError::Cancelled)` when the future was destroyed before it finished.

**Order.** The handle becomes ready, `is_finished` becomes `true`, and a waiting joiner is woken only **after the task's future has been destroyed**.

**`abort()`** asks the task to stop.

- It does not touch the future itself. At the task's **next run**, the future is dropped **without being polled** and the result is `Cancelled`.
- If the task is idle, `abort` gets it scheduled. A task already waiting for a run is not scheduled twice.
- If the task is **being polled** when `abort` is called: if that poll returns `Ready`, the output stands. If it returns `Pending`, the task is scheduled again, and that run cancels it.
- On a finished task, `abort` does nothing: the result stays. Calling it twice is fine.

**A task that can never run again.** Dropping the handle does not stop the task. The handle also does **not keep the task alive**. If the scheduler throws the task away unfinished and no waker for it is left, the future is destroyed and the handle resolves to `Cancelled`.

**The handle.** While pending, it wakes the waker from its **latest** poll. Polling it again after it returned `Ready` **panics**. `JoinHandle<T>` is `Send` when `T` is.
