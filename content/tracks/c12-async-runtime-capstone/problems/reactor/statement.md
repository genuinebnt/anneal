> **Spoiler before you open the editor.** The starter holds reference code for **capstone steps 1 to 6**, which is the whole multi-threaded runtime.
> It also solves these preparation parts. Solve them first:
>
> - **"Queue a job once, and lose no request"**
> - **"Say how a future ended, after it has cleaned up"**
> - **"The current one, on this thread"**
> - **"Go to sleep without missing work"**
> - **"One thread that rings every alarm"** (largely: the timer driver)
>
> You may replace the code with your own; keep the public names.

**Capstone, step 7 of 7.** A server's tasks mostly wait for bytes. One thread asks the operating system which sources have news and wakes the tasks that wait for them. That thread is the **reactor**.

**The pretend operating system.** Real code uses `mio` over epoll or kqueue. The test runner has neither `mio` nor a real network for this, so the starter includes `sim`, which you should read and **keep unchanged**:

- `Poller::wait` blocks until there are events or an `interrupt`, and takes the events, oldest first;
- a `Source` is told to report to a poller under a `token`;
- `Socket`s are non-blocking, with small buffers. Like edge-triggered epoll, they report a **change** once: "data arrived in an empty buffer", "a full buffer has room again", "the peer shut down". Registering reports what is already true.

**`Reactor`**

- `Reactor::start(poller)` starts one thread that waits on the poller and wakes tasks. Events for tokens it doesn't know are ignored.
- `register(source)` calls `source.register(poller, token)` with this poller and a token **never used before** by this reactor. It returns an `Io`.
- Dropping the `Io` calls `source.deregister()` and forgets the token. `registered()` counts the live ones.
- `Reactor` is `Send + Sync`, and so is `Io<S>` for a `Source` `S`. The futures below are `Send`.

**Waiting.** `poll_read_with(cx, op)` calls `op(source)`, a non-blocking attempt. `poll_write_with` does the same for writing.

- It calls `op` at least once. A value or an error is returned as `Ready`, except these two:
- `Interrupted`: call `op` again.
- `WouldBlock`: return `Pending` only if this waker is **sure** to be woken by the next event of this **direction**. That includes an event the reactor handled after the `op` call started. You may call `op` again instead.
- A `readable` event concerns waiting reads, a `writable` event waiting writes. The two never disturb each other.
- **Several tasks may wait in the same direction** of one `Io`. One event wakes **all** of them.
- Polling again with the same waker must not store another copy. After 1,000 pending polls, at most **two** copies of that waker are alive inside.

`op` is the caller's code. It may take long, and events keep arriving while it runs. **While `op` runs, hold no lock that the reactor thread or any other operation needs**, and hold none while waking. The reactor must go on serving every source, including this one. One test, from inside `op`, makes this direction and another source report news, then waits until the reactor has woken the other one.

**On `Io<Socket>`:**

- `read(buf)` gives `Ok(n)` with at least one byte, or `Ok(0)` when the peer has shut down and nothing is left. An empty `buf` gives `Ok(0)` at once.
- `read_exact(buf)` fills the whole buffer, or fails with `UnexpectedEof`.
- `write_all(data)` writes all of it, or fails with the socket's error.
- `shutdown_write()` ends this side's stream.
- Dropping a pending `read` loses no data.

**Dropping the `Reactor`** stops and joins its thread, also when it is idle. If no `Io` is left, nothing of the reactor still holds the poller when drop returns. An operation that **would have to wait** then returns an error of kind `Other`, and operations that were already waiting are woken so they get it. An operation that can succeed at once still succeeds. `Io`s may outlive the reactor.
