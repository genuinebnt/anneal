A server needs "do this in 30 seconds" thousands of times: expire a session, retry a request, give up on a peer. One thread per alarm would be absurd. You want **one** thread that sleeps until the next alarm is due.

Write `AlarmClock`. `AlarmClock::start()` starts one thread. `clock.alarms()` gives an `Alarms` handle that is `Clone + Send + Sync`; any thread uses it.

- `alarms.at(when, f)` runs `f` on the clock's thread **no earlier than** the `Instant` `when`, and returns an `AlarmId`. A time in the past is due at once. `after(delay, f)` is `at(Instant::now() + delay, f)`.
- Alarms run **one at a time**, in order of their time. Alarms with the same time run in the order they were set.
- An alarm set for **earlier** than the one the thread is waiting for must still run on time, without waiting for the later one.
- `cancel(id)` returns `true` when the alarm had not started and now **never will**. In that case the closure, and everything it captured, **has been dropped** when `cancel` returns. Otherwise it returns `false`.
- `pending()` is the number of alarms that have not started and were not cancelled.
- **No lock is held** while an alarm runs, or while a closure is dropped. Alarms, and the destructors of what they capture, may call `at`, `cancel` and `pending`.
- An alarm that **panics** does not stop the clock.

**Stopping.** Dropping the `AlarmClock` stops it.

- If an alarm is running, drop waits for it to end. The thread has exited when drop returns.
- Alarms that have not started never run. They have been **dropped** when drop returns.
- Drop returns promptly even if the next alarm is hours away.
- After that, `at` drops the closure at once and returns an id whose `cancel` is `false`.

You never drop an `AlarmClock` from one of its own alarms. The tests check "not early" and that things happen at all. They never expect an alarm to be on time to the millisecond.
