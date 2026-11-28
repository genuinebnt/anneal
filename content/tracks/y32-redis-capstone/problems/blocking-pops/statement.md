**Capstone, step 6 of 10.** A worker wants the next job as soon as there is one, without asking again and again. So a pop can now **wait**. The starter holds the reference code of steps 1 to 5, and `Waiting`, which keeps track of who waits for what.

**Spoiler warning.** The starter gives away steps 1 to 5 of this capstone. It also gives away work from these problems. Solve them first if you have not.

- *A value inside a value, arriving in pieces*: step 1 shows how to parse nested input in pieces
- *Requests that arrive in pieces*: step 1 shows the same need-more outcomes
- *Throw out what has gone stale*: `Deadlines` solves it
- *Who is in 40th place?*: `Ranked` solves its core, with `f64` scores
- *Which names fit the pattern?*: `glob_matches` solves it
- *Hold requests until something arrives*: `Waiting` solves most of it

**`BLPOP key [key ...] timeout`** and **`BRPOP`** pop from the left or right of the **first** key, in argument order, that holds a list. The reply is `[key, value]`. Keys are checked in order: another kind before a list is the wrong-kind error. Otherwise the client is **blocked**, and gets nothing yet.

- `timeout` is a number of seconds, possibly with a fraction or an exponent. Not a finite number: `ERR timeout is not a float or out of range`. It is checked before the keys.
- Its value comes from the digits as written, not from an `f64`. Digits that are all 0 mean forever. A minus sign with any other digit: `ERR timeout is negative`.
- The deadline is `now` plus the timeout in milliseconds, rounded up: `4.03` and `403e-2` are 4030, and `0.0005` and `1e-400` are 1. In `f64`, `4.03 * 1000.0` is a little above 4030, which would round up to 4031, and `1e-400` is 0, which would mean forever. A deadline too far for `u64` must not overflow or panic.
- Inside `EXEC` a blocking pop never waits: with nothing to pop it replies `NullArray`.

**Waking.** After a command, or after a whole `EXEC`, every key that **changed** and now holds a list can serve waiters. This includes a list that arrived by `RENAME`.

- Waiters are served one at a time, longest waiting first, while the list has elements. Each gets `[key, value]` from its own end.
- Changed keys are served in the order they changed.
- A waiter is served **before** the next command of whoever pushed.
- A served pop is a change like any other, so it dirties watches.

**Held commands.** While blocked, a client may send more commands. `request` keeps them, `QUIT` included, and returns nothing for them.

- When the client is answered, they run in order, as if sent then. One that blocks keeps the rest held.
- First serve every waiter that the changed keys allow. Then run the held commands of the woken clients, in the order the clients were woken.
- After each held command, serve whoever it enables, before the next one. Its effects may wake further clients.

All deliveries this causes are returned by the call that caused it, in `request` or `tick`. Each client's deliveries must be in order. The order **between** different clients is free.

**`tick(now)`** answers `NullArray` to every blocked client whose deadline is `now` or earlier. Deliveries for these come earliest deadline first, then longest waiting. Their held commands then run as above.

**`blocked()`** is the number of blocked clients. `disconnect` removes a blocked client and its held commands: it takes no element and gets no reply.

The new commands need at least two arguments after the name. There may be tens of thousands of blocked clients: no push, tick or disconnect may look through all of them.
