A scheduler that works is half a scheduler. This stage is about how it **ends** and how it behaves when its disk does not. A database that exits with writes still queued has lost data; a worker thread that dies on one bad request leaves every later request waiting forever. Both are failures you can test for without a real broken disk, using a disk that is built to misbehave.

> [!CHECK] You drop a `DiskScheduler` that still has a hundred requests queued, and nobody waited for their futures. What should be true when the drop returns? Which two things must `Drop` do, and in which order, and what goes wrong if you swap them?
> ||All hundred requests have reached the disk. `Drop` must first tell the worker to stop **after** the work already queued (a stop signal put at the back of the queue), and then wait for the worker thread to finish (`join`). Joining first would wait for a thread that was never told to stop, and hang; stopping without joining would let the drop return while the worker still has requests, and the process may exit and lose them.||
>
> - Where in the queue does the stop signal go, and why there?
> - What does `JoinHandle::join` wait for?
> - What field type lets `Drop` take the handle out of `&mut self`?

## The task

- **Drop.** Dropping a `DiskScheduler` finishes every request scheduled before it, stops the worker thread, and only then returns. An idle scheduler drops without hanging.
- **Disk errors.** If the disk returns an `Err`, the request's future gets that error, unchanged in meaning (its message is still readable).
- **A panicking disk.** If the disk **panics**, the future gets an `Err` ("the disk panicked" or similar), and the worker thread **keeps going**: later requests are still served. One bad request must not kill the scheduler.
- `deallocate_page(page_id)` asks the disk to delete that page, once.

The tests include a property: for any sequence of writes the disk sees them **in the order they were scheduled**, with none lost and none repeated.

## Your freedom

The stop signal (the `None` of `Channel<Option<_>>`, a flag, closing a channel), how the worker is found again in `Drop`, and how a panic is turned into an error. The one rule from the contract is the order: queued work first, then stop, then join.

## The Rust toolbox

**`Drop` is the shutdown hook.** `impl Drop for DiskScheduler { fn drop(&mut self) { ... } }` runs when the scheduler goes out of scope, on every path, including early returns and panics. Anything the type owns, like a thread handle, is available as `self.field`.

**`Option::take` moves out of `&mut self`.** `join` consumes the handle (`fn join(self)`), but `drop` has only `&mut self`. Store `background_thread: Option<JoinHandle<()>>` and write `if let Some(handle) = self.background_thread.take() { handle.join().unwrap(); }`. `take()` swaps in `None` and gives you the owned handle; that is the whole trick.

**`catch_unwind` at a boundary.** `std::panic::catch_unwind(AssertUnwindSafe(|| disk.write_page(id, &data)))` runs the closure and returns `Err(payload)` if it panicked. `AssertUnwindSafe` is your promise that the closure's captured state is fine to use after a panic; use it only at a boundary like this one where you then discard or report the failed work. Without it the compiler refuses captures of `&mut` data ("may not be safely transferred across an unwind boundary").

**Errors that are not `Copy`.** `io::Error::other("the disk panicked")` builds an error from a message. `err.to_string()` gives its text; tests use that to see the cause. Errors move; they are not cloned.

**A deliberately bad disk is a test double.** A struct that implements `DiskIo` and fails or panics on chosen pages is a few lines (the tests do it). When you write your own crash tests in the recovery module you will use exactly this idea.

## If this is new

- [L1 Ownership & moves](/t/l1-ownership-moves): moving out of a field, why `take()` exists.
- [S1 Option & Result](/t/s1-option-result): `Option::take`, `if let`, `Result::map_err`.
- [L2 Borrowing](/t/l2-borrowing): why `drop(&mut self)` cannot call a method that wants `self`.
- The optional concepts *clean shutdown* and *panics and unwinding* are the two things to read if the words are new.
- [C2 Message passing](/t/c2-message-passing): Use it; Understand it: a worker loop that stops when told, graceful shutdown with no lost jobs, a bounded channel from `Mutex` + `Condvar`.
- L8 Error design: Errors at scale: unwind boundaries, `catch_unwind`, a panic is not an error.
- Y5 Testing & verification: Understand it; Build it: a test double that fails on purpose, timeouts so a hang is a failure, loom.

## Tests

- Dropping with 100 requests queued runs all 100 before returning; dropping an idle scheduler returns.
- A disk error reaches the caller through the future, with its message.
- A panicking disk gives an `Err`, and the next request is still served.
- Random write sequences reach the disk in order; `deallocate_page` deletes once.

## Hints

### Where does the stop signal go?

You want "everything already scheduled, then stop". If requests are taken from the front of a queue, where should the stop signal be put? What would go wrong if it jumped the queue?

### Why does the worker die on a panic?

A panic unwinds the thread's stack. If nothing catches it, the thread ends and its queue is never read again. Find the single place in the worker where the disk is called and think about what that call needs wrapped around it, and what the future should receive when it fails.

### Joining from `drop`

`handle.join()` returns a `Result`: `Err` means the thread panicked. Is it right for a `drop` to panic as well? Decide what your code does, and say why in a comment.

## Performance

Shutdown costs the time to drain the queue, which is the I/O left to do: nothing you can speed up except by not queueing so much. Catching a panic costs nothing when no panic happens (the unwinding tables exist whether or not they are used).

**Measure it.** Schedule 10 000 writes to a disk that sleeps 50 microseconds each and time how long `drop` takes. Does it match the queue length times the delay? What would a *bounded* queue change about the first line of the test?

## Experiment

Optional. Predict first, then run.

1. **No catch.** Remove the `catch_unwind` and run the panicking-disk test. Which assertion fails first, and what does the failing message tell you about what the caller saw?
2. **Drop order.** Make `drop` join before it sends the stop signal. Predict, then run the idle-scheduler test: does it hang or fail?

## Other designs

- **`None` in the queue (ours).** Ordered with the work, simple, no extra state.
- **An `AtomicBool` running flag.** The worker checks it between requests; remember that a worker asleep in `get` will not see it until woken.
- **Closing the channel.** `mpsc` receivers return `Err` when every sender is gone; the worker's loop ends by itself. The shutdown is *dropping the sender*.
- **A `shutdown(self)` method.** Explicit, can return an error, but forgettable; `Drop` is the safety net.

## In BusTub

```cpp
DiskScheduler::~DiskScheduler() {
  request_queue_.Put(std::nullopt);   // the stop signal
  if (background_thread_.has_value()) {
    background_thread_->join();
  }
}
```

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `~DiskScheduler()` | `impl Drop for DiskScheduler` |
| `std::optional<std::thread>` member | `Option<JoinHandle<()>>` |
| `try { .. } catch (...) { .. }` around the I/O | `catch_unwind(AssertUnwindSafe(\|\| ..))`, a boundary only |
| `std::nullopt` pushed to stop | `None` put in a `Channel<Option<_>>` |
| `thread.join()` | `handle.join()` (returns a `Result`) |

**Port rule:** C++ exceptions are for errors; Rust panics are for bugs. Turn an *expected* failure into an `Err`; use `catch_unwind` only where a panic must not cross a thread or an FFI boundary.

## Learn more

- [`Drop`](https://doc.rust-lang.org/std/ops/trait.Drop.html) · [`Option::take`](https://doc.rust-lang.org/std/option/enum.Option.html#method.take) · [`catch_unwind`](https://doc.rust-lang.org/std/panic/fn.catch_unwind.html) · [`io::Error::other`](https://doc.rust-lang.org/std/io/struct.Error.html#method.other)
- The Rustonomicon on [unwinding](https://doc.rust-lang.org/nomicon/unwinding.html)
