> **Spoiler before you open the editor.** The starter of this step contains reference code that solves these earlier steps:
>
> - Refuse a bad job and say why
> - Keep the jobs and accept each once
> - Post a job, read it and page through jobs
> - Run the jobs, a few at a time
>
> It also gives away much of these preparation problems. Solve them first:
>
> - One error shape for the whole API
> - Charge the card once
> - Pages that hold still while the list changes
> - Background jobs within a memory budget
> - Run one piece of work and say how it ended
>
> The code is a stand-in, because the app cannot yet carry your own code from one step to the next. You may replace it with your own; keep the public names.

**Capstone, step 5 of 8.** A client changes its mind. The job may be waiting, running, or finishing at this very moment.

**`JobStore::cancel(id)`** accepts a cancel:

- an unknown id gives `Err(ApiError::NotFound(id))`, the id as text;
- a final job gives `Err(ApiError::NotCancellable(status))`, the status as text such as `succeeded`;
- otherwise it returns `Ok(view)`. This is a **request**. The status changes later, when whoever runs the job has stopped. The returned view still shows the job as it was, `queued` or `running`. Further cancels before the change are also `Ok`.

**An accepted cancel always wins.** From the moment `cancel` returns `Ok`:

- the job ends `Cancelled`, even if its work has just finished. It gets no output and no error;
- `start(id)` returns `false` and changes nothing;
- `finish(id, ..)`, with any outcome, records `Cancelled` and returns `true`. This holds for a queued job too.

**In a `JobService`:**

- A **queued** job becomes `Cancelled` promptly, without waiting for a slot. Its `work` is never called, so its history never has `job.started`. This holds even when the cancel comes before anything else has had a chance to run.
- A **running** job's work future is dropped. Only **after** it is gone does the job become `Cancelled` and its slot become free. This holds even if the work is busy on another thread.
- Other jobs are not affected.

**HTTP.** `POST /v1/jobs/{id}/cancel` answers **202** with the view returned by `cancel`. The errors use their usual replies, and an id that is not a `u64`, `%FF` included, is `NotFound`. Another method on this path is `MethodNotAllowed`.
