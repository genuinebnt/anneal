A challenge: no walkthrough, no hints, no solution. It is extra practice and does not count towards the course.

## What to fix

`src/storage/disk/job_pool.rs` is a small pool of worker threads: `submit` queues a job, `shutdown` is meant to wait until every job that was submitted has run and then stop the workers. It looks right, and under load some jobs never run. Find the bug and fix it.

## Why

The disk scheduler of this module has exactly this shape, and a shutdown that drops queued writes loses data without any error: the program exits cleanly with pages not on disk. 'Stop' has to mean 'finish what you were given, then stop'.

> [!CHECK] Once the tests pass: what was the bug, in one sentence, and why did ordinary inputs not show it?
> ||A worker checked the stop flag before looking at the queue, so after `shutdown` it quit with jobs still queued. Taking a job first, and stopping only when the queue is empty, makes stop mean finish, then stop.||

## The contract

- `submit(job)` queues a job; false if the pool is already shut down.
- `shutdown()` returns only after every job accepted before it has finished, and no worker is left running.
- Dropping the pool shuts it down.

## Invariants

These must hold after every step, whatever the input:

- Every job accepted by `submit` runs exactly once.
- After `shutdown` returns, no job is queued and no worker is alive.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- The number of jobs that ran equals the number `submit` accepted, whatever the number of workers and the timing.
- Submitting after shutdown runs nothing.

## Examples

Worked cases (the tests include them):

```text
4 workers, 100 jobs that each add 1 to a counter; shutdown -> counter 100
submit after shutdown -> false
```

## What the tests check

- All submitted jobs run before `shutdown` returns.
- Different numbers of workers; many small jobs.
- Submit after shutdown is refused.
- Shutting down twice is fine.

## Done when

All the `s1b_c5` tests pass, and you can say in one sentence what the bug was.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Graceful shutdown](/cookbook/p/graceful-shutdown): stop accepting work, drain what is queued, then exit.
- [Disk scheduler](/cookbook/p/disk-scheduler): a disk scheduler whose drop drains queued requests before workers stop.
- [A generic async worker pool](/cookbook/p/worker-pool): a worker pool whose shutdown drains queued jobs.
