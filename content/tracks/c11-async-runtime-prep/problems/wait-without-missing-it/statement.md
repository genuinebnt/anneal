A socket, a pipe or a hardware queue often tells you "something changed" **once**, when the change happens. It does not repeat itself. A task that finds nothing, and then arranges to be woken, can be too late: the change came in between, and the task sleeps for ever.

`Readiness` is the meeting point between one such source and one waiting task.

- The source calls `readiness.ready()` each time something new happens. It may call it from any thread at any moment, **including while your `op` is running**.
- `poll_io(&readiness, cx, op)` runs inside a future's `poll`. `op` is a **non-blocking** attempt, such as a read. It returns a value, or an error. An error of kind `WouldBlock` means "nothing yet".

What `poll_io` must do:

- It calls `op` **at least once**, whether or not `ready` was ever called.
- If `op` succeeds, return `Ready(Ok(value))`.
- If `op` fails with an error other than `WouldBlock` or `Interrupted`, return `Ready(Err(error))`.
- If it fails with `Interrupted`, call `op` again.
- On `WouldBlock`, return `Pending` **only if** the waker of this poll is sure to be woken after the next news. That includes news that arrived **after the `op` call started**. You may instead call `op` again.
- With no calls to `ready` during the poll, it returns after at most **three** calls of `op`, not counting calls that were `Interrupted`. It never spins while waiting.

What `ready` does:

- If a waker from a pending poll is stored, `ready` wakes the **latest** one. It wakes it **once**: a second `ready` without a new poll wakes nothing.
- Extra wake-ups are harmless, but the tests count them only where this statement says what they must be. Waking a waker after its poll has since returned `Ready` is allowed.

Don't hold any lock while `op` runs or while waking: the tests call `ready` from inside `op`. Use only the standard library. `Readiness` is `Send + Sync`. Only one task waits on one `Readiness` at a time.
