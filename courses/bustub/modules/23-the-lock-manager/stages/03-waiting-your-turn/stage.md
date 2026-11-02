Now the lock manager waits. `LockManager::lock` takes a lock on a table or a row for a transaction and **blocks** until it can have it. The interesting part is not the waiting but the order: requests are granted in the order they arrived, and a reader that would fit with the current holders still waits behind a writer that is queued ahead of it, so a stream of readers can never starve a writer.

## The task

In `src/concurrency/lock_manager.rs`:

- `lock(txn, resource, mode)` blocks until `mode` on `resource` can be granted: every other holder is compatible, and no earlier waiting request is incompatible.
- `try_lock` does the same test and never waits (and never jumps the queue); `unlock` releases and wakes the waiters; `holders` and `waiting` show the state.
- A `Resource` is a table, or a row of a table. Locks on different resources never affect each other.

The tests: a free lock is granted at once; shared locks are granted together; an exclusive request waits for the holder; **a reader does not jump a waiting writer**; waiters are granted in arrival order; readers that queued together are granted together; unlocking what you do not hold is an error; `try_lock` never waits; and a property against a model for the no-waiting case. A test that waits for ever fails after twenty seconds with a message instead of hanging the run.

## Your freedom

The structure behind the manager: one lock and one condition variable for everything, or a lock per resource; how the waiting requests are kept; how you wake them. `Resource`, the error types and the `LockQueue` of the last stage are given for you to use or not.

## The Rust toolbox

**`Condvar::wait` in a loop.** A woken thread must check its condition again: someone else may have been granted first, and wake-ups can be spurious.

**One mutex around the whole state.** Easy to get right and plenty fast for this course; a lock per resource is an optimisation, not a requirement.

**`notify_all`.** One release can make several waiters grantable (a batch of readers); waking everyone and letting each re-check is simple and correct.

## If this is new

- [C1 Threads & shared state](/t/c1-threads-shared-state): `Mutex` and `Condvar`.
- S5 Queues & heaps: a queue of waiters in arrival order.

## Tests

- Immediate grants; shared together; an exclusive waits; no jumping a writer; arrival order; batches of readers.
- Unlock errors; `try_lock`; a model property for the no-waiting case.

## Hints

### Two questions per waiting request

Is it compatible with every holder? Is it compatible with every request ahead of it in the queue? Both must be yes; the second is what makes the queue fair.

### Who needs waking?

An unlock can free several requests at once, and only they know whether they are first now. Let every waiter check again.

### Keep the queue entry until the grant

If a request is removed from the queue before it is granted, the one behind it may be granted too early.

## Performance

Every request takes one mutex, and `notify_all` wakes every waiter on every release: fine for tens of waiters, wasteful for thousands. Per-resource condition variables would wake only the waiters of that resource.

**Measure it.** Eight threads each locking and unlocking their own row in a loop: the time grows with the threads because of the single mutex; with per-resource state it would not.

## Experiment

Optional. Predict first, then run.

1. **Let a compatible reader pass the queue.** Which test fails, and how many readers does it take for a writer to starve?
2. **Wake one waiter instead of all.** Which test hangs?

## Other designs

- **Per-resource mutex and condition variable** (a map of `Arc<Resource>`): more parallel, harder to get right.
- **Strict FIFO** (grant only the first waiter, never a batch): simpler and slower for readers.
- **Barging allowed** (readers join readers): faster, can starve writers.

## In BusTub

BusTub's `LockManager::LockTable` and `LockRow` wait on a `std::condition_variable` of the `LockRequestQueue` and grant a request when it is compatible with the granted requests and first among the waiting ones that conflict with it.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `cv_.wait(lock, [&]{ return GrantLock(request); });` | `while !grantable { st = cv.wait(st).unwrap(); }` |
| a `std::mutex` per queue | a `Mutex<State>`; the data it protects is inside |

**Port rule:** the condition is part of the state; test it in a loop under the lock.

## Learn more

- [`Condvar`](https://doc.rust-lang.org/std/sync/struct.Condvar.html) · [Gray et al., granularity of locks](https://jimgray.azurewebsites.net/papers/granularitylocks.pdf)
