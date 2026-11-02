The disk is huge and slow; memory is small and fast. The **buffer pool** is the layer between them: a fixed number of in-memory **frames**, each able to hold one 8 KiB page. Everything above it (the table heap, the B+ tree, the hash table) asks the pool for a page by id and gets bytes it can read and change, without knowing whether the page was already in memory or had to be read from disk. This is the centre of the storage engine, and this module builds it in four steps. The first has no eviction at all: it sets up the frames, the table of which page is where, and the idea of a **pin**.

> [!CHECK] `fetch_page` returns a `FrameId`, and `frame_data(frame)` gives the bytes behind a latch, instead of `fetch_page` returning a `&mut [u8; 8192]` directly. Why is the pool built that way? What could go wrong if it handed out the bytes without the pin and the latch?
> ||A bare reference has no way to tell the pool when the caller is finished, so the pool could not know the page is in use and might evict it while the caller still holds the bytes: the caller would be reading memory that now belongs to another page. The **pin** says "do not evict this"; the **latch** (a reader-writer lock on the frame) says "no one else is writing while I read". Separating "pin" (a count the pool keeps, the call is `fetch_page`/`unpin_page`) from "latch" (a lock on the frame's bytes, the caller takes it) lets many threads hold a page in memory while only one writes at a time.||
>
> - What does "pinned" protect against?
> - What does the latch protect against?
> - Who has to remember to release each?

## The task

`BufferPoolManager::new(num_frames, disk)` makes a pool of `num_frames` frames on top of `disk`; `with_replacer(num_frames, disk, replacer)` does the same with a replacement policy of your choosing (`new` uses the ARC replacer of module 1e). Every frame starts free and zeroed. In this stage nothing is ever evicted: a page that is in memory stays there, and `fetch_page` fails when no frame is free.

- `size()`: the number of frames.
- `new_page()`: a fresh page id that no caller has been given before, even across threads. It does no I/O and uses no frame.
- `fetch_page(page)`: **pins** the page and returns its frame, reading it from the disk if it is not in memory. A page that has never been written reads as zeros. If the page is already in memory it keeps its frame, and the pin count goes up. If it is not in memory and no frame is free, `None`.
- `frame_data(frame)`: the frame's bytes behind a reader-writer latch (`&RwLock<Box<PageData>>`). Meaningful for a frame you have pinned.
- `unpin_page(page, is_dirty)`: releases one pin and remembers whether the caller changed the page. `false` if the page is not in memory or is not pinned.
- `get_pin_count(page)`: the page's pin count, or `None` if it is not in memory.
- When the last pin goes, the page becomes a candidate for eviction: the replacer must be told (`set_evictable`), and told about every access (`record_access`). Evictions arrive in the next stage.

The tests run a random sequence of new, fetch, write, unpin and read operations on your pool and on a table that says what each page should contain and how many pins it has. Pin counts, frames and bytes must match at every step. They use a plain replacer written in the test file, so a bug in your ARC cannot hide a bug in your pool.

## Your freedom

Everything inside the pool: how frames are stored, how the page table is kept, what is behind which lock, how the disk scheduler is used (the pool takes a `DiskIo` and you built a `DiskScheduler` in module 1b). The signatures above fix what callers see.

## The Rust toolbox

**One lock for the bookkeeping, one latch per frame.** The page table, the free list, the pin counts and the replacer change together, so they go behind a single `Mutex<Inner>` (BusTub's `bpm_latch_`). The bytes of each frame sit behind their own `RwLock`, so that two threads working on different pages never wait for each other's data. `Vec<RwLock<Box<PageData>>>` is the shape; build it with `(0..n).map(|_| RwLock::new(Box::new([0u8; PS]))).collect()`.

**Never wait for a latch while holding the pool's lock.** A thread that holds a frame's write latch may need the pool's lock before it can let go (to unpin), so a thread that holds the pool's lock and waits for the latch can deadlock with it. The pool's own code takes the frame latch only when no one else can hold it (a frame being loaded), or after releasing the pool lock.

**A trait object in a field.** `Box<dyn FrameReplacer>` stores "some replacement policy". Its methods take `&mut self`, so it lives inside the `Mutex`: `inner.replacer.record_access(frame, page)`. The pool never says which policy; tests and `new` choose.

**Newtypes keep ids apart.** `PageId(i32)`, `FrameId(usize)`: the compiler refuses to pass one for the other, and `frames[frame.0]` is the one place you cross over.

**Lifetimes of what `frame_data` returns.** `fn frame_data(&self, frame: FrameId) -> &RwLock<Box<PageData>>` ties the result to `&self`: the pool must outlive the reference. If the compiler says "lifetime may not live long enough", the data you are returning is not owned by `self`.

**Compiler messages you will meet.** "cannot borrow `inner` as mutable more than once": you hold `&mut inner.meta[i]` while calling a method on `inner`; copy the number you need into a local first. "`dyn FrameReplacer` cannot be sent between threads": the trait requires `Send`, and so must your type.

## If this is new

- [L1 Ownership & moves](/t/l1-ownership-moves) and [L2 Borrowing](/t/l2-borrowing): especially holding a borrow of one field while using another.
- [L4 Traits & dispatch](/t/l4-traits-dispatch): `dyn Trait` in a `Box`.
- [S3 Vec & slices](/t/s3-vec-slices) and [S4 Maps & sets](/t/s4-maps-sets): `Vec` of frames, `HashMap<PageId, FrameId>`.
- [F2 Data layout](/t/f2-data-layout) for why 8 KiB pages and what a page is in memory.
- The optional concepts *buffer pool anatomy* and *pin counts and dirty pages* are the two to read if the vocabulary is new.
- [C1 Threads & shared state](/t/c1-threads-shared-state): Understand it: `Mutex<Inner>` plus a latch per frame; never wait for a latch while holding the lock; lock ordering.
- S7 Smart pointers & interior mutability: Understand it: `Box<dyn Trait>` in a field, `Arc`.
- Y5 Testing & verification: Build it: a spy test double; threads sharing a pool; running a test many times.

## Tests

- The pool reports its size; page ids are distinct, also across eight threads, and cost no frame and no I/O.
- A page nobody wrote arrives zeroed; a page that is on disk arrives as stored.
- Fetching pins and unpinning releases; pin counts are exact; unpinning an unpinned or unknown page fails.
- A page in memory keeps its frame and bytes, and different pages get different frames; a hit does not read the disk again.
- With every frame pinned another page cannot be fetched, but a page already in memory can be pinned again.
- For random sequences on a pool with room for every page, the pool matches the model.

## Hints

### What does a frame need to remember?

Besides its bytes: which page it holds, how many pins, whether it is dirty. Write that as a struct on paper, and decide which structure finds a frame from a page id.

### Where does the lock go?

List every shared thing: the page table, the free list, the metadata, the next page id, the replacer. Which of them must change together? That group goes under one lock. Which operation reads the disk, and should that be under the lock? (In this stage the simple answer is yes; the performance section asks whether it should be.)

### Pin means two things

A pin tells the pool "do not evict". It also, together with `record_access` and `set_evictable`, tells the replacer what it may choose from. After `fetch_page`, is the frame evictable? After the last `unpin_page`?

## Performance

A hit is a hash lookup and an increment under a lock: tens of nanoseconds. A miss reads 8 KiB from the disk: tens of microseconds on an SSD, ten milliseconds on a spinning disk. The pool's job is to make hits common; the replacer decides what stays.

Holding the pool's lock during that read serialises every thread behind one disk request. It is the simplest correct design and the first thing a performance-minded engine changes.

**Measure it.** Put a disk behind the pool that sleeps 100 microseconds per read, and time 8 threads fetching different pages. Then predict what would change if the lock were released during the read, and what new problems that creates (two threads fetching the same missing page at once).

## Experiment

Optional. Predict first, then run.

1. **A pool of one.** With a single frame, which sequences of fetch and unpin make sense? What does the model test say about a pool of one frame?
2. **Drop the pin on a hit.** Forget to increment the pin count when the page is already in memory. Which test fails first? What would a real system do wrong?

## Other designs

- **One mutex for the bookkeeping, a latch per frame (ours).** Simple; every operation briefly serialises on one lock.
- **A sharded page table.** Hash the page id to one of N locks; lookups for different pages run in parallel. Eviction then needs care across shards.
- **Page table without a lock (lock-free or concurrent map).** Fast lookups, much harder invariants.
- **A background writer.** A thread that writes dirty pages ahead of time so evictions find clean victims.

## In BusTub

```cpp
class BufferPoolManager {
 public:
  BufferPoolManager(size_t num_frames, DiskManager *disk_manager, size_t k_dist = LRUK_REPLACER_K, LogManager *log_manager = nullptr);
  auto NewPage() -> page_id_t;
  auto DeletePage(page_id_t page_id) -> bool;
  auto CheckedWritePage(page_id_t page_id, AccessType access_type = AccessType::Unknown) -> std::optional<WritePageGuard>;
  auto CheckedReadPage(page_id_t page_id, AccessType access_type = AccessType::Unknown) -> std::optional<ReadPageGuard>;
  auto FlushPage(page_id_t page_id) -> bool;
  void FlushAllPages();
  auto GetPinCount(page_id_t page_id) -> std::optional<size_t>;
};
```

BusTub's current interface hands out guards; this module's `fetch_page` / `unpin_page` is its simpler ancestor, which the next module wraps.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::unordered_map<page_id_t, frame_id_t> page_table_` | `HashMap<PageId, FrameId>` inside `Mutex<Inner>` |
| `std::shared_mutex` per frame in the frame header | `RwLock<Box<PageData>>` per frame |
| `std::shared_ptr<ArcReplacer> replacer_` | `Box<dyn FrameReplacer>` owned by the pool |
| `std::optional<size_t> GetPinCount` | `Option<usize>` |
| `std::atomic<page_id_t> next_page_id_` | an integer under the pool's lock (or an `AtomicI32`) |

**Port rule:** a pointer to a polymorphic base class becomes a `Box<dyn Trait>`; a mutex that guards "everything in this struct" becomes a `Mutex<Inner>` that owns it.

## Learn more

- [`Mutex`](https://doc.rust-lang.org/std/sync/struct.Mutex.html) · [`RwLock`](https://doc.rust-lang.org/std/sync/struct.RwLock.html) · [`HashMap`](https://doc.rust-lang.org/std/collections/struct.HashMap.html)
- CMU 15-445 lecture on [buffer pools](https://15445.courses.cs.cmu.edu/) · Hellerstein, Stonebraker, Hamilton, *Architecture of a Database System*, section 4.3
