Write the loop of a background worker that must survive a deployment: `run_worker(jobs, shutdown, grace, handle)` runs
`handle(job)` for one job at a time, in the order received.

It stops in one of two ways:

- **All senders are dropped:** it finishes everything queued and returns. No deadline applies.
- **`shutdown` is cancelled:** it
  1. **at once** closes the channel, even if a job is running, so later `send`s fail and the sender keeps its job;
  2. keeps running the current job and then the queued ones, until `grace` has passed **since the cancellation**
     (one deadline for the whole drain);
  3. at the deadline, drops the running job's future and starts nothing more.

The `Report` accounts for **every job that was accepted**:
- `completed`: jobs whose future finished, in order;
- `interrupted`: the job whose future was dropped at the deadline, if any;
- `unstarted`: accepted jobs that were never passed to `handle`, in order.

If `shutdown` is already cancelled when the worker starts, queued jobs are still drained within `grace`.
The tests never put a job's finish exactly on the deadline.
