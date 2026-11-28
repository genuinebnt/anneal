> **Spoiler before you open the editor.** The starter of this step contains reference code that solves these earlier steps:
>
> - Refuse a bad job and say why
> - Keep the jobs and accept each once
> - Post a job, read it and page through jobs
> - Run the jobs, a few at a time
> - Cancel a job
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

**Capstone, step 6 of 8.** A client should not have to poll. It wants to hear about each change of its job as it happens, and to carry on where it left off after a lost connection.

**`GET /v1/jobs/{id}/events`** answers 200 with Server-Sent Events (`text/event-stream`). An unknown id, or one that is not a `u64` (`%FF` included), is `NotFound`.

Each history entry is one event: `id` is the entry's id, `event` its name, and `data` its data.

- With the request header `Last-Event-ID: n`, only entries with an id above `n` are sent. A missing header, or one that is not a `u64`, means 0.
- The response comes **at once**, whatever the job is doing.
- First come the entries that already exist. Then each new entry follows **as soon as** it is written, whether a worker or a direct store call wrote it.
- Every entry above `n` arrives **exactly once**, in order, however late the client reads.
- When the job is final and everything has been sent, the body **ends**. For a final job with nothing above `n`, it is empty.
- A `Last-Event-ID` beyond the current history of an unfinished job waits for entries above it.
- While nothing happens, send an SSE comment (a line starting with `:`) at least every 15 seconds.
- Any number of clients may follow a job. A client that never reads affects nothing.

All earlier routes and rules behave as before.

**For the tests.** `JobStore::on_nothing_new(hook)` stores a hook. Only the first one given is kept. A follower calls it each time it has read its job's history and found nothing new: right after that read, as the very next thing, with no lock held. If the hook changes the job from another thread, the follower must still send that entry. The tests use it to act in the moment between a follower's look and its wait. The hook may take a while.
