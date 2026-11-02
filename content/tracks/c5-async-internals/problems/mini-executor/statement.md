Build a single-threaded executor. `spawn(fut)` wraps the future in a task, queues it, and returns a `JoinHandle<T>` that is
itself a `Future` (awaiting it yields the task's output; `take()` reads the output after `run`). `run()` polls queued
tasks **first-woken, first-polled** until every spawned task has finished, including tasks spawned from inside tasks.

A task's `Waker` must re-queue **that task** and notify the executor, from any thread: a task waiting for a timer thread
or another thread's channel is woken while `run` sleeps on a `Condvar`. `run` returns only when no task is unfinished (not
merely when the queue is momentarily empty). `yield_now().await` lets other tasks run once.

Also provide `Executor: Clone` (clones share the queue, so a task can spawn more tasks).
