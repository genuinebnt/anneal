**Capstone, step 8 of 10.** A restart must not lose data. The server keeps an **append-only file**: a log of what changed, from which it rebuilds itself. The starter holds the reference code of steps 1 to 7, the `Disk` trait with a real directory (`DirDisk`), and the `AofError` type.

**Spoiler warning.** The starter gives away steps 1 to 7 of this capstone. It also gives away work from these problems. Solve them first if you have not.

- *A value inside a value, arriving in pieces*: step 1 shows how to parse nested input in pieces
- *Requests that arrive in pieces*: step 1 shows the same need-more outcomes
- *Throw out what has gone stale*: `Deadlines` solves it
- *Who is in 40th place?*: `Ranked` solves its core, with `f64` scores
- *Which names fit the pattern?*: `glob_matches` solves it
- *Hold requests until something arrives*: `Waiting` solves most of it

**What goes in the log.** `Core::take_log()` returns the records for everything since the last call, in order. A record is one command, as a list of arguments. Replaying the records must rebuild the keys **without knowing when they were first run**.

- A command that changes a key is recorded **as the client sent it**, letter case included, except as below. A command that changes nothing is not recorded.
- `SET` with `EX`, `PX` or `PXAT` that sets: `SET key value PXAT t`, with the deadline `t` as a number. Any `NX` or `XX` is dropped.
- `EXPIRE`, `PEXPIRE`, `PEXPIREAT`: `PEXPIREAT key t`, or `DEL key` if the key was removed at once.
- A key removed because its deadline passed, by any command or by `cycle`: `DEL key`, **before** the record of the command that found it, if that makes one.
- A blocking pop, answered at once or later: `LPOP key` or `RPOP key`, after the record of whatever enabled it.
- An `EXEC` that makes **two or more** records puts `MULTI` before them and `EXEC` after them. Pops served after it come after that `EXEC`.

These rewritten records use upper-case names. `FLUSHALL` on no keys changes nothing. Records for `LTRIM` calls that cut nothing are optional.

**The file** is named by `AOF`. It holds each record in the form of `encode_command`, one after the other.

**`Aof::write(records)`** adds them with **one** call to `append`. With `SyncPolicy::Always` it then calls `sync`. With no records it calls nothing.

In the `Disk` promise, a synced file keeps its bytes **and its name** through a power cut. So the given `DirDisk::sync` syncs the file and then its directory, because `append` may have just created it. The tests cannot prove that part of `DirDisk`: they cut the power only of a disk in memory, where a new name is always kept. Nothing here has been tried against a real power cut.

If a call fails, return `AofError::Io`. From then on every `write` returns `AofError::Broken` and touches nothing, empty ones too.

**`Aof::open(disk, policy)`** reads the file, builds a new `Core` from it and returns both. A missing file is an empty one. The core has nothing to log.

- Nothing expires during replay: the log says itself what was removed. Deadlines apply again afterwards.
- A **torn end** is a last record that is not all there, as `parse_command` says it. A last transaction without its `EXEC` is torn too. Cut off the torn end with `truncate`, so that new records follow cleanly. Call `truncate` only if there is something to cut, and make no other call that changes the disk.
- Records of the log may be larger than what clients may send. Allow values and collections far beyond `Limits::default()`.
- Anything else that is wrong is `AofError::Corrupt(offset)`, and the file is not changed. This covers:
    - a record that does not start with `*`, or that `parse_command` refuses;
    - an empty command, `MULTI` inside a transaction, or `EXEC` outside one;
    - a command that replies with an error.
- `offset` is where the bad record starts. For a failing command inside a transaction, any offset within the transaction will do.
- A failed disk call is `AofError::Io`.
