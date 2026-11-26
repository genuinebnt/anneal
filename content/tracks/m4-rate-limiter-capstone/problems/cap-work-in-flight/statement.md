**Capstone, step 5 of 7.** A rate limit admits 100 requests a second. If each now takes a minute, 6,000 are running at once. This part of the lab caps the work **in flight**. No time is involved.

## What to write

`Gate::new(limit)` makes a gate. Clones of a gate are the **same** gate, shared between threads. No method ever waits.

- `try_acquire()` gives a `Permit` if fewer than `limit` are held. Dropping the permit gives its place back, however the drop happens, a panic included. A permit may be sent to another thread and may outlive every `Gate` handle.
- `try_acquire_when(allow)` is for callers with their own rule, for example "background work only while the gate is under half full". `allow(in_flight, limit)` is asked only when there is room.
- `set_limit(n)` changes the limit at any time. Permits already out stay valid. No new one is given until fewer than the new limit are held.
- `limit()`, `in_flight()`, and `peak()`: the most permits ever held at once.

## The guarantee

This holds with any number of threads. Look at **the moment a permit is given**. There is room, and `allow` has said yes to exactly that `in_flight` and that `limit`.

`allow` may be asked more than once; only its last answer counts. It may call methods of the same gate, including `try_acquire` and `set_limit`. Do not hold a lock while calling it. The tests use this to make "something changed in the middle" happen at an exact point.
