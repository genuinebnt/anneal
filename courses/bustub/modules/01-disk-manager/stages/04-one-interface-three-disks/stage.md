Everything above the disk manager will not talk to a `DiskManager`. It will talk to *a disk*: something that can read, write and delete pages. Tests will give it a disk that lives in memory; benchmarks will give it a disk that never runs out; production will give it the real file. This stage is where the course learns to write code against an interface instead of a type, which is the idea behind every pluggable component you will build.

> [!CHECK] `DiskIo` is declared as `trait DiskIo: Send + Sync` and its methods take `&self`. What would break, concretely, if they took `&mut self`? What does `Send + Sync` let a caller do?
> ||With `&mut self` only one place can hold the disk at a time, so it could not be shared between threads through an `Arc`: every layer above (the scheduler's worker thread, the buffer pool, many query threads) needs shared access. `Send` lets the disk be moved to another thread; `Sync` lets `&disk` be used from several threads at once. Together they are the compiler-checked version of "this is thread-safe"; the locks inside are what make it true.||
>
> - Who holds a reference to the disk while a worker thread is running?
> - What does `Arc<T>` give you, and what does it not give you?
> - Where does the mutation happen if the method takes `&self`?

## The task

`DiskIo` (given, in `disk_manager.rs`) lists what a disk can do: `read_page`, `write_page`, `delete_page`. Make three things implement it and work through it.

- `DiskManager` is a `DiskIo`: the trait's three methods behave exactly like `DiskManager`'s own.
- `copy_page(disk, from, to)`: reads page `from` through whatever disk was passed in and writes it as page `to`. Both pages exist afterwards and hold the same bytes.
- `DiskManagerMemory::new(capacity)`: a disk of `capacity` pages kept in memory. A page id outside `0..capacity` is a bug in the caller: it **panics** with a message saying the disk ran out of space. A fresh disk reads as zeros. `get_num_writes()` counts writes.
- `DiskManagerUnlimitedMemory::new()`: a disk with no limit; pages spring into existence when first written; reading a page that was never written gives zeros and **does not create it**. `get_num_writes()` counts writes and `get_memory_usage()` is the bytes held, one page (8 192) per page id that has been written.

The property that ties the stage together: **all three disks agree with the same model.** The test runs one random sequence of writes, reads and deletes on the file disk, the fixed memory disk and the unlimited memory disk, and each must give the answers a plain `HashMap` of pages would (reads of deleted pages aside, as in 1a-02).

## Your freedom

How the memory disks store their pages (one big buffer, a vector of boxes, a map), whether you use a lock or something finer, and whether `copy_page` buffers on the stack or on the heap. Whether to make helpers generic or use `dyn DiskIo` is yours too; the signatures you were given decide it.

## The Rust toolbox

**A trait is an interface; `impl Trait for Type` is the agreement.** `impl DiskIo for DiskManager { ... }` says "a `DiskManager` can be used wherever a `DiskIo` is wanted". Inside the impl you can call the type's inherent methods: `DiskManager::read_page(self, page_id, buf)` (the full path avoids calling yourself).

**`dyn Trait`: a disk chosen at run time.** `&dyn DiskIo` or `Box<dyn DiskIo>` is a pointer plus a table of the trait's methods (a *vtable*, the same thing as C++'s virtual dispatch). A `Vec<Box<dyn DiskIo>>` can hold a file disk and two memory disks side by side. The alternative, a generic `fn f<D: DiskIo>(d: &D)`, makes a copy of `f` per type, faster and bigger; the course uses `dyn` here because the disk is chosen at start-up and calls are dwarfed by I/O.

**`Send + Sync` as supertraits.** `trait DiskIo: Send + Sync` means every implementor must be safe to share across threads, and the compiler enforces it: put a `RefCell` in your memory disk and `impl DiskIo for it` stops compiling with "`RefCell<..>` cannot be shared between threads safely". Read that message as "you need a `Mutex`".

**Sparse storage.** `Vec<Option<Box<[u8; 8192]>>>` indexed by page id: `None` means "never written", a `Box` keeps the 8 KiB array on the heap instead of inside the vector, and `vec.resize_with(n, || None)` grows it. `get_or_insert_with(|| Box::new([0; 8192]))` creates a page the first time.

**Asserting a precondition with a message.** `assert!(id < capacity, "page {id} on a disk of {capacity} pages: ran out of disk space")` panics with that text. A test can catch a panic with `std::panic::catch_unwind` or mark a test `#[should_panic(expected = "...")]`.

## If this is new

- [L4 Traits & dispatch](/t/l4-traits-dispatch), the first four problems: defining a trait, implementing it, `dyn` versus generics.
- [L5 Generics & associated types](/t/l5-generics), the first two: what a trait bound means.
- [S4 Maps & sets](/t/s4-maps-sets) or [S3 Vec & slices](/t/s3-vec-slices) for the storage you choose.
- Y5 Testing & verification: Understand it: a fake behind an injected trait; a model test with proptest.

## Tests

- `copy_page` copies one id to another on each of the three disks and leaves the source alone.
- A fixed memory disk panics on an id outside its capacity; an unlimited disk reads zeros without creating the page, counts writes and reports its memory.
- For random sequences, the file disk, the fixed disk and the unlimited disk each agree with the same model.

## Hints

### What is the contract, not the code?

Write down in one sentence per method what a disk promises: what `write_page` guarantees about a later `read_page` of that id, what `read_page` returns for an unknown id, what `delete_page` may or may not do. Three implementations that keep these promises are interchangeable, and that is the whole point of the trait.

### Memory is a disk with a different lock

Your memory disks have the same job as the file disk and the same sharing rule: many threads, `&self`. What holds the pages, and what protects it? Notice that nothing about the file layout survives: the design questions of 1a-01 mostly disappear, which is why test doubles are cheap.

### Reads must not create

An unlimited disk that creates a page on read would make `get_memory_usage()` grow when nobody wrote anything, and would make a "read-only" workload look like a write. Keep read and write paths separate, and let a missing page and a zero page be the same thing to the caller.

## Performance

A memory disk read or write is a lock, a `memcpy` of 8 KiB and an unlock: around a microsecond. That is why the layers above are tested on memory disks (thousands of times faster than the file, and deterministic). The cost of a `dyn` call is one indirect jump, a few nanoseconds, invisible next to a copy of 8 KiB.

**Measure it.** Time 1 million writes+reads of random pages on the memory disk through `&dyn DiskIo` and through a generic `D: DiskIo`. Predict the difference first; then see how it compares to one `pwrite` on the file disk.

## Experiment

Optional. Predict first, then run.

1. **A fourth disk.** Write a `FaultyDisk` that wraps any `DiskIo` and returns an `io::Error` on the Nth write. Your wrapper must itself be a `DiskIo`. Which tests of the layers above could you write with it? (You will need exactly this for crash testing in the recovery module.)
2. **Generic or dyn.** Change `copy_page` to take `&impl DiskIo`. Does the test still compile? What does it stop you doing with `Vec<Box<dyn DiskIo>>`?

## Other designs

- **One big `Vec<u8>` (fixed disk, ours).** A single allocation, simple arithmetic, a hard capacity.
- **`Vec<Option<Box<Page>>>` (unlimited, ours).** Allocates only what is used; a million-id sparse workload is fine.
- **`HashMap<PageId, Box<Page>>`.** The simplest of all, with hashing on every access; best when ids are very sparse.
- **A sharded lock.** A fixed array of `Mutex`es, indexed by `page_id % N`, lets threads on different pages run in parallel.

## In BusTub

```cpp
virtual void WritePage(page_id_t page_id, const char *page_data);   // DiskManager's virtual methods
virtual void ReadPage(page_id_t page_id, char *page_data);
virtual void DeletePage(page_id_t page_id);
class DiskManagerMemory : public DiskManager { /* overrides them */ };
```

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `virtual void WritePage(...)` in a base class | a `trait DiskIo` with `fn write_page(&self, ...)` |
| `class DiskManagerMemory : public DiskManager` | `impl DiskIo for DiskManagerMemory` (no inheritance: only the interface) |
| a `std::shared_ptr<DiskManager>` passed around | `Arc<dyn DiskIo>` |
| `BUSTUB_ASSERT(id < capacity, "ran out of disk space")` | `assert!(id < capacity, "...")` or an `Err` |

**Port rule:** a C++ base class with virtual methods becomes a trait; a pointer to the base becomes `&dyn Trait`, `Box<dyn Trait>` or `Arc<dyn Trait>`.

## Learn more

- The Rust Book: [defining shared behaviour with traits](https://doc.rust-lang.org/book/ch10-02-traits.html) and [trait objects](https://doc.rust-lang.org/book/ch18-02-trait-objects.html)
- [`std::panic::catch_unwind`](https://doc.rust-lang.org/std/panic/fn.catch_unwind.html) · [`Vec::resize_with`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.resize_with)
