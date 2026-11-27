A service runs other people's futures. For each one it must record how it ended, so that someone who is waiting can be told. The future may finish, panic, or be thrown away half done. The person waiting must not be told "it is over" while the future still holds its files and locks.

Write `watched(future, report)`. It returns a future with output `()` that drives `future` and calls `report` **exactly once** with an `Ending`:

- `Finished(value)`: the future returned `Ready(value)`.
- `Panicked(message)`: polling it panicked. The message is the panic's text when the panic carried a `&str` or a `String`, and `"non-string panic"` otherwise.
- `Dropped`: the watched future was dropped before the inner one finished, even if it was never polled.

**Cleanup comes first.** When `report` is called, the inner future has **already been destroyed**.

**Destructors can panic too.** If destroying the inner future panics:

- after it returned `Ready`, the ending is `Panicked(destructor's message)`, not `Finished`;
- when the watched future is dropped early, the ending is `Panicked(destructor's message)`, not `Dropped`;
- after a panic in `poll`, the ending keeps the message from `poll`. (A destructor that panics then must not escape either.)

**No panic escapes** from the inner future's `poll` or destructor: not out of `poll`, and not out of dropping the watched future. This holds even if it is dropped while the thread is already unwinding from another panic.

- When the inner future finishes or panics, `poll` calls `report` and then returns `Ready(())`, in that same poll.
- While the inner future is pending, `poll` returns `Pending` and passes on the context it was given.
- Polling again after `Ready(())` **panics**, and `report` is not called again, not even when the finished watched future is dropped.
- The inner future need not be `Unpin`. `Watched` is `Send` when the future and the report function are.

A panic inside `report` itself is not tested.
