A test that sleeps "long enough" for another thread to get somewhere fails on a busy machine and wastes time on a fast one. A test that talks to a real socket cannot choose where the bytes are cut. To test a server properly you need two tools that let the test decide.

**`Gate`** stands between the code under test and the rest of its work. Clones share one gate.

- `pass()`: the code under test calls it and waits there until it is let through.
- `release(n)`: lets `n` more calls of `pass` through, whether they are waiting already or arrive later. Exactly `n`.
- `open()`: lets every call through, now and for ever.
- `arrived()`: how many times `pass` has been called so far (waiting or not).
- `wait_for_arrivals(n, timeout) -> bool`: waits until `arrived() >= n` and returns `true`, or returns `false` once `timeout` has passed without that. It returns as soon as the n-th arrival happens.

**`Script`** is a pretend connection: `Script::new(events)` implements `Read` and `Write`, so code written for a stream can be tested against exact input.

- Reads follow the events in order. `Data(bytes)` is delivered by one or more reads: a read gets at most what fits in its buffer, and the next read gets the rest. Bytes of two events never arrive in one read. `Quiet` makes one read fail with `WouldBlock`; `Interrupt` makes one fail with `Interrupted`. `End`, and running out of events, make every later read return `Ok(0)`. A read must return `Ok(0)` only for the end (or an empty buffer, which takes nothing).
- Writes are collected; `written()` returns them. After `Script::limit_writes(n)`, one `write` call takes at most `n` bytes (`n >= 1`).
- `unread()` is the number of data bytes not yet delivered.
