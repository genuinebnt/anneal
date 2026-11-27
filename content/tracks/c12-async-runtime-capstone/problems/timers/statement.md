> **Spoiler before you open the editor.** The starter holds reference code for **capstone steps 1 to 3** (tasks, join handles and the one-thread runtime).
> It also solves these preparation parts. Solve them first:
>
> - **"Queue a job once, and lose no request"**
> - **"Say how a future ended, after it has cleaned up"**
> - **"The current one, on this thread"**
>
> You may replace the code with your own; keep the public names.

**Capstone, step 4 of 7.** Tasks need to wait for time: to retry later, to give up, to do something every second. A test that waited for real seconds would be slow and unreliable, so the runtime gets a clock that moves only when told.

**The clock.** `rt.clock()` is the runtime's `ManualClock`. It starts at zero. `advance(d)` moves it. Clones share it and may be used from any thread, including inside tasks.

**Sleeping.** All of these use the **current** runtime and panic outside one.

- `now()` is the clock's time. `sleep(d)` has the deadline `now() + d`, fixed **when `sleep` is called**. `sleep_until(t)` has the deadline `t`.
- A `Sleep` is ready on any poll at or after its deadline, **never before**. `sleep` of zero is ready on its first poll.
- While pending, it wakes the waker from its latest poll once the time has come. `Sleep` is `Send` and `Unpin`.
- Timers fire when the runtime looks: in `run_until_stalled` and `block_on`.
- Due sleeps are woken in order of **deadline**. Equal deadlines: in the order in which they **first returned `Pending`**.
- `rt.pending_timers()` is the number of sleeps that have returned `Pending`, whose time has not been handled and that still exist. **Dropping a `Sleep` removes it at once.**

**The runtime.**

- `run_until_stalled` now fires due timers and runs tasks until the queue is empty and no timer is due. It does not move the clock.
- `block_on` also sleeps while only timers are pending. **`advance` from another thread wakes it.**
- **Tasks that keep re-queuing themselves must not stop due timers from firing**, in either method.
- All of step 3's rules still hold.

**`timeout(d, future)`** gives `Ok(output)`, or `Err(Elapsed)` when the time runs out first. The deadline is fixed when `timeout` is called.

- Each poll polls the future **first**. So a value that is ready exactly at the deadline wins.
- **Before it returns `Ready`**, the inner future has been destroyed and the timeout's own timer removed. This holds even if the caller keeps the finished timeout future around.

**`interval(period)`** panics if `period` is zero. `tick().await` returns the time the tick was **due**.

- The first tick is due when `interval` was called. Tick `k` is due `k - 1` periods later.
- Late ticks are not skipped: they complete one after another, at once.
- Dropping a pending `tick()` future loses no tick.
