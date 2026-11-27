A points service must not lose a change it has confirmed, so each change is saved to disk before the caller hears "done". Saving is slow, and the cost is mostly **per save**, not per change. A thousand callers should not need a thousand saves.

Write `start(journal, max_batch)`. It spawns a Tokio task that **owns** the totals, and returns a `Handle` and the task's `JoinHandle`, which gives the final totals. `max_batch` is at least 1.

**`Handle::send(request)`** sends a request and waits for its answer. `Add(name, delta)` adds to a total (wrapping) and answers the new total. `Get(name)` answers the total, 0 if none. `Handle` can be cloned.

Each request is sent when its future is **first polled**, and requests are served in the order sent.

**Batches.** When the owner is free and requests are waiting, it takes the oldest, plus all that are **already waiting**, up to `max_batch` requests in all. It never waits for more to arrive.

- It applies the batch in order. A `Get` sees the changes before it in the same batch.
- If the batch has any `Add`, it calls `journal.commit` **once**, with all the batch's `Add`s in order. A batch of `Get`s commits nothing.
- It answers only **after** the commit has returned.

**Failure.** If `commit` fails, every request in that batch, reads included, gets `NotSaved`. The owner then stops.

Waiting and later requests get `Stopped`, including any sent after a caller learns of the failure. The final totals hold **only what was saved**: an entry for each name with a saved `Add`, and no other.

**Ending.** When every `Handle` is gone, the owner serves what was already sent, then ends. A caller that drops its future after sending does not undo its request.

`commit` is an ordinary blocking function; you may call it from the owner's task. The tests run mostly on a single-threaded runtime with its clock paused, and send many requests before the owner gets to run.
