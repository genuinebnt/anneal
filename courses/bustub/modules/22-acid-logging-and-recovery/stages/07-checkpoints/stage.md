Redo from the start of the log gets slower every day the database lives. A **checkpoint** bounds it: write every dirty page to disk, then log a `Checkpoint` record listing the transactions that are active at that moment. Everything logged before it is already in the pages, so redo can start at the **last checkpoint** instead of at the beginning. This course uses a **sharp** checkpoint (nobody writes while it runs: the simplest correct kind). Two things need care. The order: the log goes first (the write-ahead rule), then the pages, then the checkpoint record is appended and made durable; a checkpoint record that is durable while its pages are not would be a lie that recovery believes. And the **active** transactions: their changes before the checkpoint are in the pages and not skipped by redo, so if they never finish, undo must still find those changes, which are *before* the checkpoint in the log.

> [!CHECK] The log reads: `Begin T1, Change(T1), Checkpoint{active: T1}, Change(T1), Begin T2, Change(T2), Commit(T2)` and the crash. Where does redo start, which changes does it apply, and which does undo remove? What goes wrong if the checkpoint record is flushed to the log but one of the pages was not written yet? And what if `Checkpoint` did not list T1, and T1's `Begin` were before the point where recovery starts reading?
> ||Redo starts at the checkpoint and applies the second change of T1 and T2's change (the first one is already in the pages). Undo removes **both** of T1's changes: it reads the log back to T1's first record (T1 is a loser: no end record). If a page was missing when the checkpoint record became durable, redo would skip changes that are not in that page and recovery would lose them for good. If the checkpoint did not name T1 and recovery only read forward from the checkpoint, it would never see T1's `Begin` and could not know it was unfinished; its first change would stay in the pages: the losers' list is the checkpoint's second job.||
>
> - Which transactions does the checkpoint record list?
> - Why must the log be flushed before the pages?
> - Why does the checkpoint record itself need a flush?

## The task

In `src/recovery/store.rs`, `Store::checkpoint(&self) -> io::Result<Lsn>`: with writers locked out: flush the log; then write every page (flush the log up to its last change first, as `flush_page` does); then append `LogRecord::Checkpoint { active }` with the **sorted** ids of the active transactions, flush the log and return the record's LSN.

In `src/recovery/recover.rs`, in `analyse`: `redo_from` is the **index of the last `Checkpoint` record** (0 if there is none); the transactions the checkpoint lists count as seen (so one whose `Begin` and changes are all before the checkpoint, and that never finishes, is a loser).

(`undo` reads all of the records, not just those after the checkpoint, so a loser's earlier changes are found.)

The tests: exact scenarios (a checkpoint writes the log and every page; it names the active transactions; analysis starts redo at the last checkpoint; a transaction active at the checkpoint is a loser even if its begin is not read; recovery after a checkpoint redoes only what came after it; a transaction that spans a checkpoint is rolled back whole; one that began before and committed after is kept), and a property: **runs with checkpoints at random moments**: recovery leaves exactly the committed records, **the same as recovery that ignores the checkpoints** (redo from the start), and redoes no more changes than were logged after the last checkpoint.

## Your freedom

How you stop writers during the checkpoint (hold the store's lock for the whole call is the intended design), and whether to write only dirty pages.

## The Rust toolbox

**One guard for the whole operation.** Take the store's `Mutex` guard at the top and keep it: no `change` can run until the checkpoint returns.

**Position of the last match.** `records.iter().rposition(|(_, r)| matches!(r, LogRecord::Checkpoint { .. }))` finds the index of the last checkpoint; or keep a running `redo_from = i` as you scan.

**`matches!`.** A boolean test of a variant without binding anything.

**Sorting for a stable record.** `let mut active: Vec<_> = map.keys().copied().collect(); active.sort();` makes the record independent of hash order.

**Errors that are I/O.** `checkpoint` returns `io::Result`: a failed flush must fail the checkpoint, not be ignored.

## If this is new

- [C1 Threads & shared state](/t/c1-threads-shared-state): holding a lock for a multi-step operation.
- S6 Iterators: `rposition`, scanning with an index.
- The optional *ARIES recovery* concept.
- Y5 Testing & verification: Build it: crash a disk at a random point, then recover; a model of the committed state.

## Tests

- A checkpoint writes log and pages and lists the active; analysis finds the last one; redo is bounded; spanning transactions; before-and-after commits.
- Property: checkpoints change the amount of redo, never the result.

## Hints

### The order is log, pages, record

Flush the log, write the pages (each flushing the log to its LSN is then free), and only then append and flush the checkpoint record.

### Analysis still scans everything

Losers are collected from the whole log (a loser's `Begin` may be anywhere); only `redo_from` moves.

### The index, not the LSN

`redo` takes an index into the records slice; the checkpoint's position in `records` is what `analyse` returns.

## Performance

A sharp checkpoint stops all writes while every dirty page is written: with a big buffer pool that is a stall measured in seconds. **Fuzzy** checkpoints write pages in the background and log which pages were dirty and the oldest LSN a dirty page still depends on; recovery then starts at that LSN. The frequency is a trade: more checkpoints, shorter recovery, more write traffic.

**Measure it.** Run a workload of 100 000 changes with a checkpoint every 1 000, 10 000 and never; time recovery.

## Experiment

Optional. Predict first, then run.

1. **Checkpoint without writing the pages.** Log the record only. Which test fails?
2. **Start redo after the checkpoint record's following record.** Off by one: which test notices?
3. **Forget the active list.** Which scenario breaks?

## Other designs

- **Sharp checkpoint (ours).**
- **Fuzzy checkpoint with a dirty page table and a redo start LSN** (ARIES).
- **Incremental checkpoints** (RocksDB, Flink): only what changed since the last one.
- **No checkpoints at all:** recovery always replays the whole log (fine for small logs, impossible for a database that has run for years).

## In BusTub

BusTub's 2025 projects stop before recovery: Project 4 is concurrency control. The lectures on logging and recovery (CMU 15-445) teach the write-ahead log and ARIES, and BusTub's older years had a `LogManager` and `LogRecovery` that students filled in. This module is new in this course: a small store that logs before it writes, and recovery you can crash at every step.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::unique_lock<std::mutex> l(latch_)` for the whole checkpoint | `let inner = self.inner.lock().unwrap();` held to the end of the function |
| `std::find_if(rbegin, rend, ..)` | `iter().rposition(..)` |
| a dirty page table and an active transaction table in the record | the list of active transaction ids |

**Port rule:** a scoped lock stays a guard bound to a variable for the length of the work.

## Learn more

- ARIES paper, section "Checkpoints" · PostgreSQL's [checkpoints](https://www.postgresql.org/docs/current/wal-configuration.html)
