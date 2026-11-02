A database appends a record to the log for every change and cannot afford to sync the disk for each. So the **log manager** keeps recent records in a buffer in memory, hands each its **LSN** (log sequence number: here, the byte offset where the record starts in the whole log), and writes the buffer to the log device in one go when somebody needs it durable: a commit, or a page about to be written. The line between "appended" and "durable" is the most important line in the module: a crash erases everything on the wrong side of it, and every promise the database makes ("committed means safe") depends on putting it in the right place.

> [!CHECK] Three records are appended, the log is flushed, a fourth is appended, and the machine crashes. Which records does the restarted log manager see, which LSN does its next record get, and what must it do if the device holds half of a fifth record? Why is an LSN-below-the-durable-end test enough to say "this record is durable"?
> ||The first three; the next record's LSN is where the third ends (the numbering continues, because LSNs are offsets in one log that survives restarts); a half record is a torn tail: the manager must cut it away *before* appending anything, or new records would follow garbage and be unreadable. Records are contiguous and a flush writes everything buffered, so every record that starts before the durable end ends before it too.||
>
> - What does `flush_to(lsn)` do when `lsn` is already durable?
> - Does `records()` show buffered records?
> - What is `end_lsn()` for?

## The task

In `src/recovery/log_manager.rs` (the `LogIo` trait, an append-only device that is durable when `append` returns, is given; `records()` is given: it parses what the device holds):

- `LogManager::new(io)`: reads what is already in the log, finds where the last intact record ends (`parse_log`), **truncates a torn tail** (`io.truncate`) and starts there.
- `append(&self, record) -> Lsn`: puts the record's bytes at the end of the buffer; its LSN is the offset where it starts in the whole log (durable part plus buffer).
- `flush(&self)`: writes the buffer to the device in one `append` call and empties it; nothing is written if the buffer is empty.
- `flush_to(&self, lsn)`: makes the record that starts at `lsn` durable; does nothing if it already is.
- `flushed_lsn()`: where the durable log ends: every record that starts below it is durable. `end_lsn()`: where the next record will start.

The tests: exact scenarios (an LSN is where the record starts; unflushed records do not survive a crash; flush moves the durable end to the end of the log and a flush with nothing buffered writes nothing; `flush_to` writes only when needed; after a restart the log continues after the last intact record; a torn tail is cut away before new records are added), and a property: **any run of appends, flushes, `flush_to`s, crashes and torn crashes**: the durable log is exactly the records appended before the last flush, in order, the LSNs are the sums of the sizes, and after a crash that cuts the tail the log is a prefix.

## Your freedom

How you hold the state (one `Mutex` around a struct, two atomics and a lock), how you represent the buffer (a `Vec<u8>` of serialized records, or records until the flush), and whether `flush_to` flushes everything or only enough.

## The Rust toolbox

**One lock around the state.** `Mutex<State>` with `buffer: Vec<u8>` and `durable: u64`: `append` and `flush` lock it, so two threads appending never interleave bytes and every LSN is unique.

**A trait object for the device.** `io: Arc<dyn LogIo>` is shared with the test's crashable disk; calling `io.append(&bytes)?` propagates the I/O error to the caller.

**`std::mem::take`.** `let bytes = std::mem::take(&mut state.buffer);` empties the buffer and gives you its contents to write, in one step.

**`io::Result`.** The functions that touch the device return `io::Result<..>`: a failed write must not be reported as durable.

**Offsets, not counters.** Because the LSN is a byte offset, `append`'s answer is `durable + buffer.len()` before the push: no separate counter to keep in step.

## If this is new

- [C1 Threads & shared state](/t/c1-threads-shared-state): a `Mutex` around a struct, the lock as the critical section.
- [L4 Traits & dispatch](/t/l4-traits-dispatch): `Arc<dyn LogIo>`.
- L8 Error design: `io::Result` and `?`.
- The optional *write-ahead logging* and *durability* concepts.
- [F7 I/O & serialization](/t/f7-io-serialization): Storage formats: write-ahead-log records: a length, a checksum, a body; a torn tail is the end of the log.
- S9 I/O & filesystem: Understand: append-only files, `fsync`, what is durable when a call returns.

## Tests

- LSNs; losing the buffer in a crash; flush and the durable end; `flush_to` writes only when needed; restart continues the numbering; a torn tail is cut.
- Property: random appends, flushes, crashes and torn crashes against a model of what is durable.

## Hints

### Update the durable end only after the write succeeded

If `io.append` fails, the buffer is still the only copy of those records: keep it and do not move `durable`.

### On restart, trust only what parses

`parse_log` gives the intact length; if the device is longer, cut it to that length before you append, or the first new record will be unreadable.

### `flush_to` is a convenience

It is enough to compare `lsn` with the durable end and call `flush` if it is not below it.

## Performance

A flush is one device write (and one sync in a real system): the cost that decides commit latency. Records appended by other threads while a flush is in flight can join the next one, which is the idea of **group commit**: many transactions' commits share one sync. This implementation holds the lock across the write, which is simple and serialises flushes; the group-commit version releases it while the device works.

**Measure it.** Append a million 40-byte records flushing after every record, every 100 and never; compare writes to the device and total time.

## Experiment

Optional. Predict first, then run.

1. **Forget the truncate.** Skip the cut of the torn tail. Which test fails, and what does the log look like to the next reader?
2. **Count the syncs.** Make `flush_to` flush even when the record is durable. Which test notices, and how much would that cost a real commit path?

## Other designs

- **A buffer and one lock (ours).**
- **Group commit with a background flusher thread** and a condition variable: committers wait until the flusher passes their LSN.
- **A ring buffer of log pages** with the log writer owning disk I/O (PostgreSQL's WAL writer).
- **LSN as a counter** with an index from LSN to offset (needed when records are compressed).

## In BusTub

BusTub's 2025 projects stop before recovery: Project 4 is concurrency control. The lectures on logging and recovery (CMU 15-445) teach the write-ahead log and ARIES, and BusTub's older years had a `LogManager` and `LogRecovery` that students filled in. This module is new in this course: a small store that logs before it writes, and recovery you can crash at every step.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::mutex latch_; std::vector<char> log_buffer_;` | `Mutex<State>` where `State` owns the buffer |
| `disk_manager_->WriteLog(buf, size)` | `io.append(&buf)?` |
| `std::atomic<lsn_t> persistent_lsn_` | a `u64` inside the mutex, or an atomic |
| a flush thread and `std::condition_variable` | optional: `Condvar` for group commit |

**Port rule:** a mutex plus the data it guards becomes a `Mutex<T>` that owns the data.

## Learn more

- [`Mutex`](https://doc.rust-lang.org/std/sync/struct.Mutex.html) · PostgreSQL's [WAL configuration (group commit, `commit_delay`)](https://www.postgresql.org/docs/current/wal-configuration.html)
