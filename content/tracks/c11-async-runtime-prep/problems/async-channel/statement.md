A thread that waits for room in a full queue can block. A task must not: the thread under it has other tasks to run. So a waiting task returns `Pending`, and someone has to wake it later.

Write `channel(capacity)`: a bounded queue between async tasks, with any number of senders and one receiver. Use only the standard library. There is no executor in your code; the tests poll your futures themselves.

**The queue**

- `capacity` is at least 1; `channel(0)` panics.
- The queue never holds more than `capacity` values. `len()` says how many it holds.
- `send(v).await` finishes once `v` is in the queue. If there is room and no other `send` is waiting, it is ready on its first poll.
- `recv().await` gives the oldest value in the queue. It waits while the queue is empty and a sender exists.
- Values from one sender arrive in the order that sender sent them.

**Ends going away**

- `Sender` is `Clone`. Once **every** sender is dropped, `recv` still gives what is queued, then `None`, every time it is called.
- Once the receiver is dropped, `send` returns `Err(v)` with its own value, including a `send` that was already waiting. `is_closed()` is then `true`.
- Values still in the queue are **dropped together with the receiver**, even while senders are alive. A value's destructor may use the channel: it may hold a `Sender` of this very channel.

**Waking**

- A pending future must wake the waker from its **latest** poll.
- A pending `recv` is woken when a value arrives and when the last sender goes away. A pending `send` is woken when the receiver goes away.
- Room: **whenever the queue has room and some `send` futures are waiting, at least one of them has been woken** since it last returned `Pending`. This must stay true when other waiting `send` futures are dropped, before or after they were woken. Waking more futures than needed is allowed. Which sender gets the free place is up to you.

**Cancelling.** Any of these futures may be dropped while it is pending, as happens to the losing side of a `select`.

- A dropped pending `send` puts nothing in the queue.
- A dropped pending `recv` loses no value.

**Memory.** Polling a pending future again with the same waker must not store it again. After 1,000 such polls, at most **two** copies of that waker may be alive inside the channel.

`Sender<T>` and `Receiver<T>` can move between threads when `T: Send`, and so can the futures from `send` and `recv`. Never call `wake` while holding a lock.
