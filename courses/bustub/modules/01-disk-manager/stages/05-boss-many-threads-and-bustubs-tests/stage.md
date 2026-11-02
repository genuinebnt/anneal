**Where this fits.** The last stage of the module: stress your design with threads, then run BusTub's own test.

> [!CHECK] `write_all` returned `Ok` and the process then crashes, or the machine loses power. Is the data on disk? What does `shut_down` add, and what would you have to call to be sure after a *commit*?
> ||Not necessarily: `write_all` hands the bytes to the operating system's page cache; they reach the drive later. `shut_down` calls `sync_all` on both files, which asks the OS to write them to stable storage. A commit that must survive power loss needs a sync of the *log* before the commit is acknowledged, every time, which is why databases batch commits behind one sync.||
>
> - Where do the bytes sit after `write_all` returns?
> - What does `sync_all` ask for that `write_all` does not?
> - Which file must be durable before a commit is reported?

## The task

Nothing new to design. Make the disk manager hold up under pressure, and finish the two remaining public pieces.

- `shut_down()`: makes everything written so far durable (both files are synced). After it, a new `DiskManager` over the same files finds the log as it was left.
- **Threads.** The disk manager is shared through an `Arc` by many threads at once. Whatever you built must survive: threads writing their own pages all read them back; a reader never sees half of one write and half of another (a *torn page*); counters from concurrent writes, deletes and log appends are exact; the log has every appended byte.
- **BusTub's own test.** `tests/disk_manager_test.rs` is `disk_manager_test.cpp` ported test for test, using only the public API: `ReadWritePageTest`, `ReadWriteLogTest`, `DeletePageTest` (100 pages written, then 200 written and deleted: the file must not grow) and `ThrowBadFileTest` (a bad path is an `Err`).

If something fails here that passed earlier, the earlier tests missed a case: the failing run prints the thread count and the page.

## Your freedom

The same as before: the design is yours. If a stress test fails, the fix may be small (a lock held across two steps instead of around each) or structural; either is fine as long as the properties hold.

## The Rust toolbox

**Sharing across threads.** `let dm = Arc::new(DiskManager::new(p)?);` then `let dm = Arc::clone(&dm);` for each thread and `thread::spawn(move || { ... })`; `handle.join().unwrap()` waits and propagates a panic. `move` transfers the clone into the closure.

**Stop flags.** A shared `AtomicUsize` (or `AtomicBool`) a writer sets when done and readers poll lets readers loop "until the writer is finished" without a lock.

**Making a hang a failure.** A concurrency bug often shows up as a test that never ends. Run the work in a thread and wait on a channel with `recv_timeout`, so a deadlock is a failed test with a message instead of a stuck CI job.

**`sync_all` versus `sync_data`.** `File::sync_all` flushes contents and metadata (`fsync`); `sync_data` flushes contents and only the metadata needed to read them (`fdatasync`). On macOS Rust asks for `F_FULLFSYNC`, which also flushes the drive's own cache.

## If this is new

- [C1 Threads & shared state](/t/c1-threads-shared-state) if you have written it already; otherwise the examples in the `mutex-owns-its-data` concept are enough.
- [L3 Lifetimes](/t/l3-lifetimes), first problem: why `thread::spawn` needs `move` and `'static`.
- S9 I/O & filesystem: Use; Understand (I/O errors): open and create files, positional reads and writes, `io::Result`, `sync_all`.
- [L1 Ownership & moves](/t/l1-ownership-moves): Clones & drops: `Drop` and RAII: a file closes when its owner goes away.
- Y5 Testing & verification: Understand it: a fake behind an injected trait; a model test with proptest.

## Tests

- Threads writing disjoint pages read them back, and counters are exact afterwards.
- A concurrent reader never sees a page that is a mix of two writes.
- Concurrent writes, deletes and log appends keep every counter exact and the log complete; `shut_down` makes the log available to a new manager.
- BusTub's four tests.

## Hints

### Which pair of operations must be one step?

If a stress test fails and you do not know why, name the two steps of one operation that another thread can slip between: looking something up and acting on what you found, reading a counter and adding to it, finding the end of the log and writing at it.

### Shrink the failing case

Run with two threads and ten operations first. A race that needs 8 threads and 2 000 operations to show up usually shows up in a much smaller case; the smaller case has a readable message.

### Locks and I/O

Holding a lock while you write to the file makes your design simple and serial. That is allowed here. What it costs is the question for the experiment below.

## Performance

With one lock around all file I/O, 8 threads do no more work than 1: they take turns. That is correct and is what BusTub does; the disk scheduler above you provides the parallelism, by queueing requests for one worker. A finer design (a lock per page, or no lock for the lookup) lets independent pages proceed in parallel, at the price of more places for a bug to hide.

**Measure it.** Time 8 threads writing 8 distinct pages each 20 000 times, against 1 thread doing all 160 000. Predict first. Run it with and without `sync_data` after each write, and read what that does to the ratio.

## Experiment

Optional. Predict first, then run.

1. **Take the lock out of the read path.** Read the page table under a lock but do the file read outside it. Which test breaks, and what sequence of operations produces the wrong answer? (This is the classic "time of check to time of use" race.)
2. **Count the syncs.** Call `shut_down` after every write in a loop of 1 000. How long does it take, per call, on your machine? Convert to commits per second and compare with what a database advertises.

## Other designs

Your concurrency design is the interesting comparison: one lock for everything (ours), one lock for the table and one per page, lock-free reads using `Arc` snapshots of the table (readers never block, writers copy), or a dedicated I/O thread with a queue (which is exactly what the disk scheduler of the next module is).

## In BusTub

```cpp
void DiskManager::ShutDown() {
  { std::scoped_lock scoped_db_io_latch(db_io_latch_); db_io_.close(); }
  log_io_.close();
}
```

BusTub's `ShutDown` only closes the files; the port also syncs them, which is what makes the log survive a power cut.

## The C/C++ way

| gtest / C++ | Rust |
|---|---|
| `TEST(DiskManagerTest, ReadWritePageTest)` | `#[test] fn read_write_page_test()` |
| `std::thread t(f); t.join();` | `thread::spawn(move \|\| f())` and `.join().unwrap()` |
| `EXPECT_EQ(size, dm.GetDbFileSize())` | `assert_eq!(size, dm.get_db_file_size())` |
| `ASSERT_THROW(DiskManager("/no/such/dir/x"), Exception)` | `assert!(DiskManager::new(path).is_err())` |

## Learn more

- [`File::sync_all`](https://doc.rust-lang.org/std/fs/struct.File.html#method.sync_all) · [`sync_data`](https://doc.rust-lang.org/std/fs/struct.File.html#method.sync_data) · [`thread::spawn`](https://doc.rust-lang.org/std/thread/fn.spawn.html)
- CMU 15-445 lecture "Database Storage I" (in the module resources below) and the [Project 1 page](https://15445.courses.cs.cmu.edu/fall2026/project1/)
