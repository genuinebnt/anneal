Two more tools for tasks that must wait: one limits how many may go at once, the other lets one task tell another "look again".

Write both with the standard library only. The tests poll your futures by hand.

**`Semaphore::new(n)`** has `n` permits.

- `acquire().await` gives a `Permit`. At most `n` permits exist at any time. Dropping a `Permit` gives it back.
- If a permit is free and no other `acquire` is waiting, `acquire` is ready on its first poll.
- `try_acquire()` never waits. It returns a permit if one is free and no `acquire` is waiting, and `None` when all `n` are held. In other cases it may return either.
- `available()` is `n` minus the permits that exist.
- **Progress:** suppose fewer than `n` permits exist and some `acquire` futures are waiting. Then at least one of them has been woken. Polling the woken ones gives at least one of them a permit. Which one is up to you.
- Dropping a pending `acquire` never loses a permit and never strands the others, whether or not it had been woken.

**`Signal::new()`** carries a "something happened" message. It remembers **at most one** unused signal.

- `signal()` never waits. `wait().await` finishes by using up the remembered signal.
- A signal sent while nobody waits is **remembered**: the next `wait` is ready on its first poll.
- One signal never completes two waits. Signals that arrive while one is still unused **merge with it**: after two `signal()` calls with no `wait` finishing in between, only one `wait` finishes.
- If waits are pending when `signal()` is called, at least one of them is woken, and polling the woken ones completes exactly one.
- If a woken `wait` is **dropped before it is polled**, the signal is not lost. Another pending wait is woken, or has been already. A later `wait` can use it too.

**Both:**

- A pending future must wake the waker from its **latest** poll.
- Polling a pending future 1,000 times with the same waker must leave at most **two** copies of that waker alive inside.
- Both types are `Send + Sync`, and their futures are `Send`. Never call `wake` while holding a lock.
