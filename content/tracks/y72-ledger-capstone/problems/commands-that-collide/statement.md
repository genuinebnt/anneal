**Capstone, step 3 of 8.** Many callers send commands. Replies get lost, so commands arrive twice. And two callers can work from the same balance. The starter holds working code for steps 1 and 2.

**`fold(account, line)`** applies a stored line: its event, and the fact that its command id, with its fingerprint `print`, was used on this account.

**`try_commit(stream, loaded, id, command)`** makes one attempt, from an account that may be out of date:

1. If `loaded` has used `id`: return `Duplicate` when the fingerprint equals `fingerprint(command)`, or is 0. Otherwise `Domain(IdReused)`. Nothing is written.
2. Ask `decide(loaded, command)`. If it refuses, return the refusal **only if** the stream is still at `loaded.version`. If it has moved, return a `Store(Conflict)`.
3. Append the events, expecting version `loaded.version`. Each line carries `id`, the command's fingerprint, and `self.now` as `ts`.
4. Return `Applied(number of events)`.

**`execute`** loads and calls `try_commit`. While the result is a conflict, it loads again and retries, as often as it takes. Anything else is returned.

An id is used **per stream**. `Client`, `Reaction` and `Bounce` ids with the same number are different ids. A refused command is not remembered.

**`state_at(stream, version)`** is the account right after the line with that stream version. Version 0 is a new account.

**Snapshots.** `Ledger::new(store, n)` keeps, per stream, a copy of the account. When a successful `try_commit` carries a stream's version to or past a multiple of `n`, the new account becomes its snapshot. `n = 0` means never. `snapshot_version` says which version it covers.

**`load(stream)`** must call `read_stream` **once**, for the lines after the snapshot's version (0 without one). It always equals `state_at(stream, u64::MAX)`.
