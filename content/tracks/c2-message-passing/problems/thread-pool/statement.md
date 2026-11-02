`ThreadPool::new(n)` starts `n` worker threads that take boxed jobs from one FIFO queue.
`execute(f)` queues a job; `submit(f)` queues it and returns a `TaskHandle<R>` whose `join()` gives the
closure's result, or `Err(TaskPanicked)` if the closure panicked.

- A job that panics must **not** kill its worker; the pool keeps its full size.
- `shutdown(self)` stops accepting work, lets the workers finish **every job already queued**, joins all
  threads and returns how many jobs panicked. Dropping the pool without calling it must do the same.
- Jobs never run on the calling thread, and the pool never starts more than `n` threads.
