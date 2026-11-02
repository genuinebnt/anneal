The disk manager is the bottom of the whole database: it turns "page 17" into bytes in a file and back. Everything above it, the buffer pool, the B+ tree, the table heap, will only ever say *write this page* and *read that page*. Get this layer right and nobody above it needs to know that files exist.

> [!CHECK] Two threads call `write_page` for the same page id, which has never been written, at the same moment. List the steps each one performs, and say what can go wrong if the steps of the two interleave. What does that tell you about where a lock must be held?
> ||Each thread (1) looks the page up, (2) finds it unknown, (3) picks a place for it, (4) records the place, (5) writes the bytes. If both do step 1 before either does step 4, both pick a place: the second record overwrites the first, so the first thread's place is lost forever (a leak), and the two writes can land in different places while readers find only one. The lookup, the choice and the record must be one critical section: hold one lock across all three.||
>
> - What does each thread know after step 2?
> - Which two steps does a lock have to keep together?
> - What would you see in the file if both threads picked the same place?

## The task

Make `DiskManager` store pages. In `src/storage/disk/disk_manager.rs` the public methods are in your repo with their documentation; this stage needs these:

- `DiskManager::new(path)`: opens the database file (creating it if it is missing, keeping its contents if it is there) and a log file next to it, named like the database file with its extension replaced by `log`. If either cannot be opened it returns the `io::Error`; it never panics.
- `db_file_name()` and `log_file_name()`: the two paths.
- `write_page(page_id, &data)`: stores one 8 192-byte page under that id. Whatever the page held before is replaced.
- `read_page(page_id, &mut buf)`: fills `buf` with the page's bytes. A page that was never written reads as all zeros, and the **whole** buffer is overwritten with them.
- `get_db_file_size()`: the database file's length in bytes right now (the next stage uses it).

What must hold, whatever you do inside:

1. **Read what you wrote.** After any sequence of writes and reads on any page ids, every read returns what the last write to that page stored, or zeros if there was none. The test checks this against a `HashMap` for random sequences.
2. **Pages are independent.** Writing one page never changes another, in any order, and page ids can be large and sparse: 0, 1, 1 000 and 1 000 000 are all fine.
3. **Many threads.** All methods take `&self`; the disk manager will be shared between threads. (Stage 5 hammers it; design for it now.)

## Your freedom

Everything else. How the file is laid out, how you remember where each page is, whether you keep a table in memory, whether one lock or several protect it, which fields the struct has (there are none to start with). The tests call only the methods above. A design that follows BusTub (a page table from page id to a slot in the file) is one good answer; the "Other designs" section shows three more.

## The Rust toolbox

**Positional I/O: read and write at an offset, no cursor.** A normal `File` has a cursor (`seek`, then `write`), which needs `&mut File` and breaks when two threads share it. On Unix the `FileExt` trait gives you `write_all_at(&buf, offset)` and `read_at(&mut buf, offset)`, both taking `&self`:

```rust
use std::os::unix::fs::FileExt;
file.write_all_at(&page, offset)?;      // like pwrite(2): writes all of it, moves no cursor
let n = file.read_at(&mut buf, offset)?; // like pread(2): may return fewer bytes than asked for
```

`read_at` can return short (and `0` means end of file), so a read of a whole page is a loop until the buffer is full or `n == 0`.

**Open without truncating.** `OpenOptions::new().read(true).write(true).create(true).truncate(false).open(path)?` creates a missing file and leaves an existing one alone. `File::create` truncates, which would destroy the database on every start.

**A method that takes `&self` but changes things needs a lock.** Rust gives you shared mutability through types that check at run time: `Mutex<T>`. `let mut state = self.state.lock().unwrap();` gives a guard that dereferences to `T`; the lock is released when the guard goes out of scope. `unwrap()` here is normal: it only fails if another thread panicked while holding the lock.

**Errors are values, and `?` is the early return.** `fn new(...) -> io::Result<DiskManager>` returns `Ok(..)` or `Err(io::Error)`. Writing `OpenOptions::new()....open(path)?` returns the error to the caller at once. A function that is allowed to fail must not `unwrap()` the thing that can fail.

**Arrays are values.** A page is `[u8; 8192]`: it is `Copy`, so `let b = a;` copies 8 KiB, and a `&PageData` parameter avoids that. `buf.fill(0)` zeroes it; `dst.copy_from_slice(&src)` copies and panics if lengths differ.

**The `HashMap` borrow trap.** `map.get(&k)` returns `Option<&V>`, a *borrow* of the map. If you hold it and then call `map.insert(...)` the compiler stops you ("cannot borrow as mutable because it is also borrowed as immutable"). End the borrow first by copying the value out: `map.get(&k).copied()`.

## If this is new

Do these on the Rust tracks first if the toolbox above reads like a foreign language; each takes a few minutes and teaches exactly one idea you need here.

- [S1 Option & Result](/t/s1-option-result), the first four problems: `?`, `map`, `ok_or`, and why `unwrap` is a decision.
- [L1 Ownership & moves](/t/l1-ownership-moves) and [L2 Borrowing](/t/l2-borrowing), the first three problems of each: what a move is, why `&` and `&mut` cannot coexist (the `HashMap` trap is this rule).
- [S4 Maps & sets](/t/s4-maps-sets), the first three: `get`, `insert`, `entry`.
- [S3 Vec & slices](/t/s3-vec-slices): slicing a buffer (`&buf[a..b]`) and `copy_from_slice`.
- S9 I/O & filesystem: Use; Understand (I/O errors): open and create files, positional reads and writes, `io::Result`, `sync_all`.
- L8 Error design: Custom errors: turn an `io::Error` into your own error; `?` through two layers.
- F1 Measure & read the machine: Measure: `Instant`, `black_box`: how to time what the Performance sections ask for.

## Tests

- A written page reads back, a rewrite replaces it, a page never written reads as zeros (the whole buffer), very large page ids work.
- For random sequences of writes and reads, every read agrees with a `HashMap` of pages, and pages written in any order do not disturb each other. A failure prints the shortest failing sequence.
- `new` creates both files, reopens an existing database file, and returns an error (not a panic) for a path that cannot be opened.

## Hints

### Is a page id a place?

A page id is a *name* handed out by the layers above; it is not a position in the file. Ids arrive in any order and may be huge. Ask what you must remember about each page so that a later read finds it, and where that memory has to live.

### Where does the lock go?

Two threads write a page that has never been written, at the same instant. Walk through what each does: find out whether the page is known, choose a place, remember it, write the bytes. Which of those steps must not interleave with the same steps of another thread, and what is the smallest region of code that covers them?

### The whole buffer, always

`read_page` is given a buffer that may hold anything (the tests fill it with a marker byte). Whatever you do for a page you have never seen, the caller must get zeros in every position, not just the part you wrote. A page is always exactly one buffer long; a file may be shorter than the place a page would live.

## Performance

A page write is one `pwrite` system call of 8 KiB and a page read one `pread`: a few microseconds when the bytes are in the operating system's page cache, which they are unless you ask otherwise (nothing here calls `fsync`; see the durability article). Looking up where a page lives should be constant time on average. If you hold one lock across the file write, as BusTub does, writers take turns: correct and simple, and the first thing a faster design would change.

**Measure it.** Write 100 000 pages (random ids in `0..1000`) from one thread and then from eight threads, timing each. Predict whether eight threads are about eight times faster, and then use `strace -c` (Linux) or `dtruss` (macOS) to count the system calls. The numbers depend on your machine; the shape of the answer does not.

## Experiment

Optional. Predict first, write the prediction down, then run it.

1. **Direct offsets.** Replace your table with `offset = page_id * 8192` (a sparse file). Does stage 1's test pass? What does `get_db_file_size()` say after writing page 100 000, and what would 1a-02 think of it?
2. **The lock's cost.** Hold the lock only for the table lookup and do the file I/O outside it. Which test fails, if any? Which sequence of operations could now go wrong?

## Other designs

After you pass, compare yours with these. None is "the right one"; each is right for some system.

- **Page table plus slots (BusTub's, and ours).** A map from page id to a slot number; slots lie end to end in the file. Pages can be reused, ids can be sparse, and moving a page is just changing the map. The map is in memory only: after a restart the file's pages are not findable. Real engines store the map on disk too.
- **Direct offset.** Page id times page size. No table, no bookkeeping, and reads and writes need no lock for lookup. But the file's size is the largest id ever used, and sparse ids waste address space; deleted pages cannot be reused. Fine when ids are dense and never reused.
- **Log-structured.** Append every write to the end of the file and keep the latest offset for each page. Writes are sequential (fast on any disk), the file only grows until a compaction pass copies live pages forward. This is the idea behind LSM trees and several storage engines.
- **Per-page locks instead of one.** A table of locks, one per page, lets threads writing different pages run in parallel; the table itself still needs protection. More code, and more ways to deadlock.

## In BusTub

```cpp
if (pages_.find(page_id) != pages_.end()) { offset = pages_[page_id]; }   // exists: overwrite in place
else { offset = AllocatePage(); }                                           // new: take a slot
...
pages_[page_id] = offset;
```

and, for the log file, in the constructor:

```cpp
log_file_name_ = db_file_name_.filename().stem().string() + ".log";   // relative to the working directory!
log_io_.open(log_file_name_, std::ios::binary | std::ios::in | std::ios::app | std::ios::out);
if (!log_io_.is_open()) { /* didn't exist: create it, or throw Exception("can't open dblog file") */ }
```

C++ opens, checks and opens again to create; `create(true)` is the whole dance in Rust, and an `Exception` becomes an `Err`. This port puts the log next to the database file, not in the working directory. BusTub's `ReadPage` allocates a slot even for a page it has never seen; this port's reads never allocate.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `db_io_.seekp(offset); db_io_.write(data, size);` on an `fstream` | `file.write_all_at(data, offset)?` |
| `std::scoped_lock l(db_io_latch_);` (a mutex next to the data it protects, by convention) | `let mut io = self.db_io.lock().unwrap();` (the mutex *owns* the data) |
| `throw Exception("can't open db file")` | `return Err(e)` or `?` |
| `std::unordered_map<page_id_t, size_t> pages_` | `HashMap<PageId, usize>` |
| `memset(buf, 0, BUSTUB_PAGE_SIZE)` | `buf.fill(0)` |

**Port rule:** a C++ method that mutates under a latch becomes a `&self` method whose mutable state sits inside a `Mutex`; an exception becomes `Result` and `?`.

## Learn more

- [`OpenOptions`](https://doc.rust-lang.org/std/fs/struct.OpenOptions.html) · [`Path::with_extension`](https://doc.rust-lang.org/std/path/struct.Path.html#method.with_extension) · [`io::ErrorKind`](https://doc.rust-lang.org/std/io/enum.ErrorKind.html)
- [`std::os::unix::fs::FileExt`](https://doc.rust-lang.org/std/os/unix/fs/trait.FileExt.html) · [`Mutex`](https://doc.rust-lang.org/std/sync/struct.Mutex.html)
- The Rust Book: [recoverable errors with `Result` and `?`](https://doc.rust-lang.org/book/ch09-02-recoverable-errors-with-result.html) · [shared-state concurrency](https://doc.rust-lang.org/book/ch16-03-shared-state.html)
- BusTub's [disk_manager.cpp](https://github.com/cmu-db/bustub/blob/master/src/storage/disk/disk_manager.cpp)
