The disk manager has two more jobs that every database layer above it relies on: it keeps count of what it has done, so tests (and operators) can see its behaviour, and it keeps a **log**, the append-only file where changes are recorded before they are applied.

> [!CHECK] A log holds 6 bytes. A caller asks `read_log` for an 8-byte buffer at offset 4, and later at offset 6. What does each call return, what is in the buffer afterwards, and why is it useful that the second call leaves the buffer untouched?
> ||Offset 4: the log has two bytes from there, so the call returns true and the buffer holds those two bytes followed by six zeros. Offset 6 is exactly the end of the log: it returns false and the buffer is exactly as the caller left it. Leaving it untouched lets the caller tell "nothing here" from "some data" without scanning for zeros: a recovery loop reads records until it gets false.||
>
> - How many bytes of the 8 does the log have at offset 4?
> - What should a reader be able to conclude from a false?
> - Why not return zeros in both cases?

## The task

Three small capabilities, all through the public methods.

- **Counters.** `get_num_writes()` counts every `write_page`, rewrites included. `get_num_deletes()` counts deletes of pages that existed; deleting something absent counts for nothing. `get_num_flushes()` counts non-empty log writes.
- **Appending.** `write_log(data)` appends `data` to the log file. An **empty** write does nothing at all: it is not counted as a flush and does not touch the file. Reopening the database keeps what was appended earlier; the log is never truncated.
- **Reading.** `read_log(buf, offset)` reads `buf.len()` bytes of the log starting at byte `offset`. If `offset` is at or past the end of the log it returns `false` and leaves `buf` untouched. Otherwise it returns `true`, and whatever part of the buffer the log does not have is filled with zeros.

Properties the tests check: the log always reads like the concatenation of everything appended (any window of it, at any offset); the counters match the operations performed, over random sequences; and the log file on disk holds exactly the appended bytes, also after a restart.

## Your freedom

Whether counters are atomics or sit under a lock, whether the log has its own lock, how you read at an offset (`read_at`, or seeking), and how you find the log's length.

## The Rust toolbox

**Atomic counters.** `AtomicUsize::fetch_add(1, Ordering::Relaxed)` increments a shared counter from any thread without a lock, and `load(Ordering::Relaxed)` reads it. For a pure counter, `Relaxed` is enough: you need the count to be exact, not to order other memory around it. (Use `Acquire`/`Release` when a flag publishes data to another thread.)

**Append mode.** `OpenOptions::new().append(true)` makes every `write` go to the end of the file, atomically with respect to the file's length, even if two handles write. `write_all(data)?` from `std::io::Write` loops until everything is written.

**The length of a file.** `file.metadata()?.len()` is the current length in bytes; use it to decide whether an offset is past the end.

**Filling a buffer in pieces.** `let n = ...; buf[n..].fill(0);` zeroes the tail after a short read. `&mut buf[..n]` is a sub-slice you can hand to `read_at`.

**`#[derive(Default)]`-style thinking for counters.** If you have several counters, group them in a small struct of atomics instead of three loose fields; a struct is also easy to expose later as a `stats()` call.

## If this is new

- [S3 Vec & slices](/t/s3-vec-slices): sub-slices `&buf[a..b]` and `fill`.
- [S1 Option & Result](/t/s1-option-result): returning `io::Result<bool>`: what the two layers mean.
- S9 I/O & filesystem: Use; Understand (I/O errors): open and create files, positional reads and writes, `io::Result`, `sync_all`.

## Tests

- Counters: rewrites count as writes, deletes count only when the page existed, an empty log write is not a flush; random sequences match a model.
- The log reads like the concatenation of the appends (random chunks, random windows, past-the-end and short reads), and the file on disk holds exactly those bytes.
- Reopening keeps the log: bytes appended in an earlier session are still readable, and new ones go after them.

## Hints

### What counts as a flush?

The word is BusTub's. Think about what an empty `write_log` should do to the file, to the counter, and to the lock, and write down the answer before you write the code.

### Past the end is an answer, not an error

`read_log` can say three different things: here are your bytes, here are some of your bytes (and zeros after them), and there is nothing at that offset. They are `true`, `true` and `false`. Which of them is `Err`?

### One window, three cases

A read at `offset` of `len` bytes is either entirely inside the log, partly inside, or outside. Handle "outside" first, then do a read that tolerates being short, then clear the rest of the buffer. Test the boundary: offset equal to the length is *outside*.

## Performance

A log append is one `write` of a few bytes; it sits in the page cache until something syncs it (see the durability article). That is a database's weak point: a commit is only durable after `fsync`, which costs milliseconds on a disk; the standard answer is to batch many appends behind one sync (*group commit*). A counter increment is one atomic add, a few nanoseconds uncontended and much more when many cores hammer the same cache line.

**Measure it.** 8 threads each increment one `AtomicUsize` 10 million times. Then give each thread its own counter in a separate cache line and sum at the end. Predict the ratio before you run it; the answer is *false sharing*.

## Experiment

Optional. Predict first, then run.

1. **Torn appends.** Open the log *without* append mode and track the end yourself with `seek` under a lock. Write a stress test that appends from 8 threads. Does it fail? What about if you forget the lock?
2. **Counters under a lock.** Replace the atomics with a counter inside the page `Mutex`. Which test notices? What does it cost in a 8-thread run?

## Other designs

- **Atomics per counter (ours).** No lock, exact, tiny.
- **Counters inside the existing lock.** One fewer concept, but the counters and the thing they count are updated together, which is the easiest way to make them agree.
- **A `stats()` snapshot.** One call returns a struct of all the numbers at one moment: a better API for tools, a small change to the trait.
- **Log with a length prefix per record.** BusTub's log is raw bytes; real logs frame each record (length, checksum), so recovery can tell a torn record from a good one. You will want this in the recovery module.

## In BusTub

```cpp
void DiskManager::WriteLog(char *log_data, int size) {
  if (size == 0) { return; }   // no effect on num_flushes_ if log buffer is empty
  num_flushes_ += 1;
  log_io_.write(log_data, size);   // sequence write
  log_io_.flush();                 // flushes the *stream's* buffer to the OS; not an fsync
}
```

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `int num_writes_` bumped under a mutex, read with no lock | `AtomicUsize`, `fetch_add` and `load` |
| `log_io_.write(data, size); log_io_.flush();` | `log.write_all(data)?` (a `File` has no user-space buffer to flush) |
| `memset(log_data + n, 0, size - n)` | `buf[n..].fill(0)` |
| `GetFileSize(name) -> int` (−1 on error, overflows past 2 GB) | `file.metadata()?.len()` as `u64` |

**Port rule:** a plain integer counter shared between threads is a data race in C++ and a compile error in Rust; reach for an atomic.

## Learn more

- [`AtomicUsize`](https://doc.rust-lang.org/std/sync/atomic/type.AtomicUsize.html) · [`Ordering`](https://doc.rust-lang.org/std/sync/atomic/enum.Ordering.html) · [`io::Write`](https://doc.rust-lang.org/std/io/trait.Write.html) · [`File::metadata`](https://doc.rust-lang.org/std/fs/struct.File.html#method.metadata)
- CMU 15-445 lecture "Database Logging" (in the module resources)
