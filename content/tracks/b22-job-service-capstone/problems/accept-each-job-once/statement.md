> **Spoiler before you open the editor.** The starter of this step contains reference code that solves these earlier steps:
>
> - Refuse a bad job and say why
>
> It also gives away much of these preparation problems. Solve them first:
>
> - One error shape for the whole API
>
> The code is a stand-in, because the app cannot yet carry your own code from one step to the next. You may replace it with your own; keep the public names.

**Capstone, step 2 of 8.** Clients retry. A `POST` whose answer was lost is sent again, and it must not create a second job. Every job also needs a history that never disagrees with its status.

Write `JobStore`, a store that many threads share. Clones are handles to the **same** store. No async runtime is involved yet.

**`submit(key, spec)`**

- A new key creates a job and returns `(view, true)`. Ids are 1, 2, 3 and so on, in order of creation.
- A known key with an **equal** `spec` returns the job **as it is now** and `false`.
- A known key with a different `spec` is `Err(ApiError::KeyReuse)`.
- Of any number of calls that race with the same key, exactly one creates.

**Status.** A job starts `Queued`. These are the only changes:

| Call | Allowed from | New status |
|---|---|---|
| `start(id)` | `Queued` | `Running` |
| `finish(id, Succeeded(output))` | `Running` | `Succeeded`, with `output` |
| `finish(id, Failed(error))` | `Running` | `Failed`, with `error` |
| `finish(id, Cancelled)` | `Queued` or `Running` | `Cancelled` |

Both return `true` when they change the job. Otherwise, unknown ids included, they return `false` and change **nothing**. `output` and `error` are `None` except as the table says.

**The clock** is a counter in the store, not the time of day. Every real change, creation included, takes the next number: 1, 2, 3 and so on, across all jobs. `created_at` is the number of the job's creation. `updated_at` is the number of its latest change. Retries and refused calls take no number and use no id.

**History.** Every change appends one entry to the job's history. Entry ids are 1, 2, 3 **per job**. The name is `job.queued`, `job.started`, `job.succeeded`, `job.failed` or `job.cancelled`. `data` is JSON text with exactly `job_id` (a number) and `status` (the new status, such as `"running"`).

`events(id, after)` returns the entries with an id above `after`, oldest first, or `None` for an unknown job. `get` returns the view or `None`.

A `JobView` serializes to JSON with its field names and a lowercase status. `Status::as_str` and `is_final` are yours to write too. The store will hold many thousands of jobs, so lookups by id and key must stay fast.
