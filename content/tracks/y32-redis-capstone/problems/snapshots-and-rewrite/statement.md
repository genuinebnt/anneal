**Capstone, step 9 of 10.** The log grows for ever: a counter increased a million times is a million records. This step adds **snapshots**, compact copies of the keys, and a **rewrite** that shrinks the log while writes go on. The starter holds the reference code of steps 1 to 8, a `crc32` function, and three new errors.

**Spoiler warning.** The starter gives away steps 1 to 8 of this capstone. It also gives away work from these problems. Solve them first if you have not.

- *A value inside a value, arriving in pieces*: step 1 shows how to parse nested input in pieces
- *Requests that arrive in pieces*: step 1 shows the same need-more outcomes
- *Throw out what has gone stale*: `Deadlines` solves it
- *Who is in 40th place?*: `Ranked` solves its core, with `f64` scores
- *Which names fit the pattern?*: `glob_matches` solves it
- *Hold requests until something arrives*: `Waiting` solves most of it
- *Come back the same after a restart*: step 8's records use its method
- *A key-value store with a write-ahead log that survives torn writes*: step 8 shows how a torn end is cut off
- *Capstone 1: records and frames* (key-value store series): the given `crc32` is its checksum, and later starters show how to decode with bounds checks

**The snapshot format.** All numbers are little-endian. A *blob* is a `u32` length and then the bytes.

```
"RSNAP1"
for each live key, in byte order:
    u8 type          0 string, 1 list, 2 hash, 3 set, 4 sorted set
    u8 has_deadline  0 or 1
    u64 deadline     only if has_deadline is 1
    blob key
    value
u8 0xFF
u32 crc32 of every byte before this one
```

A string value is a blob. The others are a `u32` count, then that many items:

| Kind | Each item | Order |
|---|---|---|
| list | blob | from the left |
| set | blob | byte order |
| hash | blob field, blob value | by field |
| sorted set | `u64` score bits, blob member | the set's order |

Score bits are `f64::to_bits`.

**`encode_snapshot(core, now)`** writes the keys that are live at `now`.

**`decode_snapshot(bytes)`** builds a core, with nothing to log. Refuse, with `BadSnapshot`, anything that is not exactly this format:

- a wrong magic or checksum, or bytes missing or left over;
- an unknown type or flag, a count of 0, or a NaN score;
- a key, member or field that appears twice.

**No input may panic.** Do not reserve memory for what a count merely claims. The checksum is no proof: the tests send bad contents with a correct checksum.

**`rewrite_commands(core, now)`** returns, in log form, commands that build the live keys. No command may have more than 130 arguments, and 5 keys of 1000 elements must take at most 100 commands. Keys with a deadline end up with it.

**`save_snapshot(disk, core, now)`** stores the snapshot as the file named `SNAPSHOT`. At every moment, a power cut leaves the old snapshot or the new one, whole. If it returns `Ok`, the new one is kept. You may use the temporary name `dump.snap.tmp`, which may be left over from an earlier crash.

**The rewrite** has two halves, so that the compact copy can be written elsewhere while the server keeps working. The caller has written all of the core's records before it starts.

- `begin_rewrite(core, now)` returns the compact commands for the keys now. If a rewrite is running: `AofError::Busy`. If the log is broken: `AofError::Broken`.
- Writes made meanwhile go to the old log **as usual**.
- `finish_rewrite(base)` replaces the log with `base` followed by everything written since `begin_rewrite`, **each record once**. With none running: `NotRewriting`. If the log broke meanwhile: `Broken`.
- However `finish_rewrite` ends, that rewrite is over. If a disk call fails, return `Io`; the log still holds everything, **keeps taking writes**, and another rewrite can be tried.
- At every moment, a power cut leaves a log that rebuilds the same keys. You may use the temporary name `appendonly.aof.tmp`, which may be left over from an earlier crash.
- `cancel_rewrite()` ends a running rewrite and does nothing otherwise.

The disk's promises are those of the `Disk` trait: appended bytes survive a power cut only after `sync`, while a `rename` is atomic and survives.

**`recover(disk, policy)`** starts the server's data.

- If the log exists and is not empty, open it as `Aof::open` does. The snapshot is not even read.
- Otherwise, if a snapshot exists, load it. A bad one is `AofError::BadSnapshot`. Before returning, write the log from it, so that later restarts keep both the snapshot's keys and new writes. A crash during this must not lose keys.
- Otherwise start empty.
