**Capstone, step 5 of 10.** Many clients now share the keyspace, and some need several commands to happen as one. The starter holds the reference code of steps 1 to 4.

**Spoiler warning.** The starter gives away steps 1 to 4 of this capstone. It also gives away work from these problems. Solve them first if you have not.

- *A value inside a value, arriving in pieces*: step 1 shows how to parse nested input in pieces
- *Requests that arrive in pieces*: step 1 shows the same need-more outcomes
- *Throw out what has gone stale*: `Deadlines` solves it
- *Who is in 40th place?*: `Ranked` solves its core, with `f64` scores
- *Which names fit the pattern?*: `glob_matches` solves it

Two things in the starter's engine are for you. `Engine::changed` lists, in order, every key that was **changed** since you last emptied it, expiry included. `check_command(args)` is the engine's refusal for an unknown name or a wrong number of arguments.

Write `Core`, the front door for all clients.

- `connect()` returns an id never used before. `disconnect(id)` forgets the client and everything about it.
- `request(id, args, now)` runs one command of that client. It returns what must be sent as `Delivery { to, frame, published, close }`. `published` and `close` are `false` unless said otherwise.

  For now there is exactly one delivery, to `id`. It returns nothing for an unknown client or empty `args`.
- `cycle(now, dice)` is `Engine::cycle`.

Command names ignore case. All commands of the earlier steps work as before. The new ones:

**`QUIT`** (any number of arguments) replies `OK` with `close` set, and disconnects the client. It does so even inside a transaction.

**`MULTI`** replies `OK`, and starts a queue. While it is open, every command except `MULTI`, `EXEC`, `DISCARD`, `WATCH` and `QUIT` is **queued** and replies `Simple("QUEUED")`. It runs only at `EXEC`, at the time of `EXEC`.

- If a command is unknown or has the wrong number of arguments, reply with that error at once. The transaction is then **spoiled**.
- A wrong number of arguments to `MULTI`, `EXEC`, `DISCARD` or `WATCH` is refused, and changes nothing about an open transaction.

**`EXEC`** ends the transaction and runs the queued commands in order, with nothing in between. It replies with the array of their replies; an empty queue gives an empty array. A command that fails while running puts its error in its place, and the others still run.

- A spoiled transaction runs nothing: `EXECABORT Transaction discarded because of previous errors.`
- A transaction whose watch failed (below) runs nothing, and replies `NullArray`. Spoiled is checked first.

**`DISCARD`** drops the queue and replies `OK`.

**`WATCH key [key ...]`** replies `OK`. If any watched key **changes** before `EXEC`, whoever changed it (the watcher included), that `EXEC` runs nothing.

- A command that changes nothing does not count. Examples:
    - a refused command, `DEL` of a missing key, or `SET` whose condition fails;
    - adding a member that is there, or removing one that is not;
    - a pop of zero, or `PERSIST` with no deadline;
    - a `ZADD` or `ZINCRBY` that leaves every score as it was.
- Writing the same string again with `SET` does count, as does `HSET`. `LTRIM` of an existing list may count either way.
- `FLUSHALL` changes every key that existed.
- Expiry counts, **even if nothing touched the key since**, when its deadline has passed by `EXEC` or a `cycle` removed it. A key already past its deadline when watched does not count for that.

`EXEC` and `DISCARD` end all of the client's watches, however `EXEC` turns out. So does **`UNWATCH`**, which replies `OK`. Inside a transaction, `UNWATCH` is queued, replies `OK` at `EXEC` and has no other effect.

**Errors:** `ERR MULTI calls can not be nested`, `ERR WATCH inside MULTI is not allowed`, `ERR EXEC without MULTI`, `ERR DISCARD without MULTI`. The first two leave the transaction as it was.

There may be many thousands of clients with watches. A write must not look at clients that do not watch that key.
