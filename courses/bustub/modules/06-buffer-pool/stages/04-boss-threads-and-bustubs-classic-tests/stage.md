**Where this fits.** The end of the textbook pool: eight threads share a small pool, the same sequence of writes goes through three different replacers, and BusTub's classic tests run.

> [!CHECK] Two threads fetch the same page, which is not in memory, at the same moment. List every state the pool can end up in if the bookkeeping is not atomic, and say which of them loses data.
> ||Both find the page missing; each takes a free frame and reads the page into it; the page table then maps the page to one frame (the other insertion overwrites the first), and one frame holds a copy nobody finds. Worse, the two threads each pin "their" frame: if both then modify their copy, one modification is invisible. The fix is that "look up, choose a frame, insert into the page table" is one atomic step under the pool's lock.||
>
> - What must be atomic?
> - What would a pin count of 1 for a page pinned twice look like in a test?
> - Which state is silently wrong rather than loudly wrong?

## The task

Nothing new to design.

- **Threads.** Eight threads, 300 increments each on random pages out of 40, through a pool of 12 frames with the ARC replacer. Each increment is a fetch, a change under the frame's write latch, and an unpin(dirty); every page must end with exactly the number of increments applied.
- **The replacer does not matter.** The same pseudo-random sequence of 600 writes through three pools (FIFO, ARC, LRU-K) must leave every page with the same bytes. The replacer changes *which* pages are in memory, never *what* they contain.
- **The model under your replacers.** The random operations of the earlier stages run once more with ARC and with LRU-K under the pool.
- **BusTub's classic tests.** `buffer_pool_manager_classic_test.rs` is the pre-guard `BufferPoolManagerInstance` tests of earlier BusTub years, ported.

## Your freedom

The same as before.

## The Rust toolbox

**Run a test many times.** `for i in $(seq 100); do cargo test --test stages_1f -- s1f_04 || break; done` finds races that happen one run in fifty.

**Thread sanitiser / loom / miri.** Beyond this course's scope but good to know exist; the *testing concurrent code* concept shows the options.

**Debug with counters.** A race is easier to see when you count: operations applied versus operations visible. The threaded test does that.

## If this is new

- Everything is in the earlier stages of this module.
- [C1 Threads & shared state](/t/c1-threads-shared-state): Understand it: `Mutex<Inner>` plus a latch per frame; never wait for a latch while holding the lock; lock ordering.
- Y5 Testing & verification: Build it: a spy test double; threads sharing a pool; running a test many times.

## Tests

- Eight threads, 2 400 increments, no update lost.
- Three replacers, one data outcome.
- The model, once under ARC and once under LRU-K.
- BusTub's classic tests.

## Hints

### The count is wrong by a few

A lost update. Which step of an increment is not atomic: the fetch, the change under the latch, the unpin? Is it possible that two threads hold the same page's frame without the latch?

### A hang

If the test never finishes, a thread is waiting for something that will never happen: look for a place that holds the pool's lock while waiting for a latch, or an `unpin` that never runs.

## Performance

Run with `--release` and count increments per second; compare with a single thread. A pool behind one mutex scales poorly: eight threads on eight cores may be no faster than one. The experiment below is the first step to fixing it.

## Experiment

Optional. Predict first, then run.

1. **Scaling.** Measure increments per second with 1, 2, 4 and 8 threads. Where does it stop scaling, and what is the limit?
2. **A sharded page table.** Split the page table into 8 parts by `page_id % 8`, each under its own lock. Which operations still need a global lock (eviction)? What did you gain?

## Other designs

None for this stage. The *Other designs* sections of 1f-01 to 1f-03 list the alternatives to compare with yours.

## In BusTub

The classic tests are `BinaryDataTest` and `SampleTest` from the older `buffer_pool_manager_instance_test.cpp`: binary data (including a NUL byte and the full byte range) written to a page, the pool filled, pages unpinned and fetched again, verifying the first page survived. Their `NewPage(&id)` returned a pinned page; here `new_page()` hands out an id and `fetch_page` pins it.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::thread` + `join()` | `thread::spawn(move \|\| ..)` + `handle.join().unwrap()` |
| `std::atomic<size_t>` for counters | `AtomicUsize` with `fetch_add(1, Ordering::SeqCst)` |
| `EXPECT_EQ(0, strcmp(buf, "Hello"))` | `assert_eq!(&data[..5], b"Hello")` |

**Port rule:** a C string in a page becomes a byte slice; compare slices, not NUL-terminated strings.

## Learn more

- [`AtomicUsize`](https://doc.rust-lang.org/std/sync/atomic/type.AtomicUsize.html) · [`thread::spawn`](https://doc.rust-lang.org/std/thread/fn.spawn.html)
