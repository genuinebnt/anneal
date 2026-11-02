Two more pieces finish the guard interface. `read_page` and `write_page` are the versions most code uses: they panic if the page cannot be brought in, because a caller that has no frame for a page it needs has no useful way to continue. And a guard can **flush** its own page: write the bytes it already holds to the disk, without asking for the latch it already has. Getting that second part wrong is the classic buffer pool deadlock, and the tests set a trap for it with timeouts.

> [!CHECK] A writer holds a page's write latch. Another thread calls `flush_page` on that page. Describe a deadlock that is possible if `flush_page` holds the pool's lock while it waits for the frame's read latch, and say what the writer is waiting for.
> ||The flusher holds the pool's lock and waits for the page's latch, held by the writer. The writer finishes and its guard is dropped, which calls `unpin_page`, which needs the pool's lock, held by the flusher. Each waits for the other: deadlock. The cure is that `flush_page` must not hold the pool's lock while it waits for a latch: pin the page under the lock, let the lock go, take the latch, write, and unpin.||
>
> - Which two locks are involved, and in which order does each thread take them?
> - What does the pin protect while the lock is released?
> - Which other call in the pool takes a latch?

## The task

- `read_page(page)` and `write_page(page)`: like `checked_read_page` and `checked_write_page`, but **panic** when the page cannot be brought in (every frame is pinned).
- `ReadPageGuard::flush()` and `WritePageGuard::flush()`: write the bytes the guard is looking at to the disk now, **without taking the frame's latch again** (the guard already holds it; asking again would wait for itself). The guard keeps its pin and its latch. A write guard that flushes is **clean** afterwards (`is_dirty()` is false); flushing a clean guard still writes.
- `flush_page(page)` from 1f-03 must be **safe against the deadlock above**: a blocked flush must not stop the rest of the pool from working. The test holds a write guard on a page, starts `flush_page` of that page on another thread, and then checks that the main thread can still fetch and unpin other pages; after the writer lets go, the flush must write the latest bytes, and must not leave the page pinned.

## Your freedom

How the guard writes the bytes (a pool method you add, the disk scheduler directly), and how you restructure `flush_page` so that the lock is not held while a latch is awaited.

## The Rust toolbox

**Panicking wrappers.** `self.checked_read_page(page).expect("every frame is pinned: no room to bring the page into memory")`: the message says what happened and how it could have been avoided. `expect` is `unwrap` with a message.

**Scoped lock to end a borrow.** `let frame = { let mut inner = self.inner.lock().unwrap(); ... frame };` releases the lock at `}` and keeps only the frame id; the next statement runs without the lock.

**Lock order as a rule.** Write down, in a comment, the order in which locks may be taken (pool lock, then nothing; or frame latch only after the pool lock is released). Deadlocks come from two threads taking two locks in opposite orders; a written order is how you check a function against the others.

**Testing a hang.** The tests run the dangerous call on a thread and wait on a channel with `recv_timeout`; if the thread is stuck the test fails with a message instead of hanging the suite. Use the same in your own tests.

**A copy of the bytes.** `let data: PageData = *self.get_data();` copies 8 KiB onto the stack (fine here); `Box::new(*data)` puts the copy on the heap for a disk request that owns its buffer.

## If this is new

- [L1 Ownership & moves](/t/l1-ownership-moves): copying a large array versus moving a box.
- [L2 Borrowing](/t/l2-borrowing): scoped blocks.
- The optional concept *deadlock and lock ordering* shows the dining-philosophers version of the problem and the standard cures.
- [C1 Threads & shared state](/t/c1-threads-shared-state): Understand it: deadlock and lock ordering; a flush that cannot deadlock.
- Y5 Testing & verification: Build it: loom or a watchdog timeout for a lock bug.

## Tests

- `read_page` and `write_page` return a guard when there is room and panic when nothing can be evicted.
- A write guard can flush its page; a clean guard still writes; a read guard can flush too; flushing while holding the latch does not hang.
- A blocked `flush_page` does not block the rest of the pool; `flush_page` still writes the latest bytes; a flushed page is not left pinned.

## Hints

### What does `flush` on a guard really need?

The bytes (the guard has them) and the page id (the guard has it) and a way to write to the disk (the pool has it). It does *not* need a latch or a frame lookup. What method on the pool would be enough?

### Restructure `flush_page`

Today (from 1f-03) it probably looks up the frame, takes the latch and writes, all under the pool's lock. Rewrite as: lock, find the frame, pin it, unlock; latch, copy; unlatch; write; unpin. What marks the page clean, and when?

### The test hangs

If the test reports a timeout rather than a failing assert, the pool is deadlocked. Find which thread is waiting for which lock; the message names the test.

## Performance

A flush through a guard is one disk write. `flush_page` is the same plus a pin and unpin. The difference matters under contention only.

**Measure it.** A writer holds a page for 50 ms; start a `flush_page` of it and measure how long unrelated fetches take in the meantime. With the old design they all wait; with the new design they do not.

## Experiment

Optional. Predict first, then run.

1. **Bring the deadlock back.** Hold the pool's lock in `flush_page` again and run the two tests; which one fails and with what message?
2. **Flush all.** How should `flush_all_pages` behave if one page is write-latched by a long-running thread? Is skipping it correct? Should it wait?

## Other designs

- **Pin, release the lock, latch, write (ours).** The classic pattern.
- **A per-frame "being flushed" flag.** Avoids a second copy; more state to keep right.
- **Flush only through guards.** Remove `flush_page` from the pool; a caller flushes what it holds. Simpler, but a checkpoint must take every latch.

## In BusTub

```cpp
void WritePageGuard::Flush();   // writes the page to disk, keeping the latch
void ReadPageGuard::Flush();
auto BufferPoolManager::FlushPage(page_id_t page_id) -> bool;   // the one to keep deadlock-free
```

A `FlushPage` that waits for the frame's latch is the one place where the pool's lock order can go wrong, which is why this stage tests it with a timeout.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `BUSTUB_ENSURE(opt.has_value(), "...")` | `.expect("...")` |
| `std::scoped_lock` released at the end of a block | a block that ends the `MutexGuard`'s life |
| acquire locks in a fixed global order by convention | the same convention; the compiler does not check it |

**Port rule:** the compiler checks data races, not deadlocks; lock ordering is a convention you write down and test.

## Learn more

- [`Result::expect`](https://doc.rust-lang.org/std/result/enum.Result.html#method.expect) · [`mpsc::Receiver::recv_timeout`](https://doc.rust-lang.org/std/sync/mpsc/struct.Receiver.html#method.recv_timeout)
- Wikipedia: [deadlock](https://en.wikipedia.org/wiki/Deadlock)
