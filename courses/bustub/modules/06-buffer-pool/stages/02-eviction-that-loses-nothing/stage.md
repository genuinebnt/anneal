A pool of 3 frames can serve a database of a million pages only if it can **throw pages out**. The rule that makes this safe is short: a frame may be reused when nobody has its page pinned; and if the page was changed, the change must reach the disk first. If you get this right the caller never knows an eviction happened: a page reads back what was last written to it, however many other pages passed through the pool in between. That sentence is the property the tests check.

> [!CHECK] A page is evicted while it is dirty, its bytes are written to disk, and the frame is reused. Between the choice of victim and the write, what could another thread do to the page? Name two things the pool's design must stop, and say what in the pool stops each.
> ||Another thread could **pin** the page (fetch it) after the replacer chose it, or **modify** it through the frame latch while the write is in progress. The first is stopped by the pool's lock: choosing the victim, removing it from the page table and starting the write happen under the lock, so a fetch must wait and then sees the page as not in memory. The second is stopped because the page is unpinned (nobody has it, so nobody can hold its latch); a pinned page is never chosen. Violating either gives a lost update or a torn write.||
>
> - What state makes a frame safe to reuse?
> - What does the replacer know, and what does the pool know?
> - Which has to be written back before the frame is reused, and why before?

## The task

The interface is the same as in 1f-01. Now, when `fetch_page` needs a frame and none is free:

- It asks the replacer for a victim (`evict`). If there is none, because every frame is pinned, it returns `None`.
- If the victim's page is **dirty** (some `unpin_page(.., true)` reported a change since it was loaded), its bytes are **written to disk** before the frame is reused. A clean page is **never** written.
- The victim is removed from the page table. The new page is read into its frame.
- Pins and dirtiness: a pinned frame is never a victim; the dirty flag is remembered across several pins (`unpin(.., true)` then `unpin(.., false)` still leaves the page dirty); it is cleared when the page is written back.

The model test runs random sequences on a pool of 3 frames and up to 24 pages, with pages written, unpinned and fetched in any order, and checks at every step: `fetch_page` fails exactly when the page is not in memory and every frame is pinned; a page that was in memory keeps its frame; every page reads back what was last written; pin counts are exact; **no page that was never modified is ever written to disk**.

## Your freedom

How the dirty flag is stored, where the victim's write happens relative to the lock (the simple answer holds the lock; the experiment explores letting go), whether you write dirty pages eagerly or only on eviction.

## The Rust toolbox

**`Option<FrameId>` from `evict()?`.** In a function that returns `Option`, `expr?` returns `None` at once when `expr` is `None`: `let first = queue.front()?;`. It keeps the no-victim path to one character.

**`Option::take` to move a value out of a struct.** `entry.owner.take()` moves the value out of an `Option` field and leaves `None` behind, in one step: no clone, and no moment where two places claim it.

**Copy before you write.** A disk request owns its buffer (module 1b), and the frame goes on being used, so the bytes are copied into a buffer of their own first. `dst.copy_from_slice(src)` copies one slice into another of the same length (it panics if the lengths differ).

**Waiting on a future.** `future.get().expect("the scheduler is running").expect("writing a page")`: the first `expect` is for a broken promise, the second for the I/O error. Decide what your pool does with an `Err` (this one panics; a database would report it).

**`|=` on a bool.** `meta.dirty |= is_dirty;` accumulates: once true, stays true until you clear it.

**Check in debug builds.** `debug_assert!(inner.meta.iter().filter(|m| m.pin_count > 0).count() <= num_frames)` is trivially true; better ones: after an eviction, the page table has no entry for the victim, and the frame is in no list. Add a `fn check(&self)` and call it in your own tests.

## If this is new

- [S1 Option & Result](/t/s1-option-result): `take`, `?`, `expect`.
- [S3 Vec & slices](/t/s3-vec-slices): `copy_from_slice`.
- [L2 Borrowing](/t/l2-borrowing): why `let meta = &mut inner.meta[i]; inner.replacer.set_evictable(..)` is an error (two borrows of `inner`) and how to reorder.
- [C1 Threads & shared state](/t/c1-threads-shared-state): Understand it: `Mutex<Inner>` plus a latch per frame; never wait for a latch while holding the lock; lock ordering.
- [F3 Memory & allocation](/t/f3-memory-allocation): Allocate less: reusing buffers, copying a page into a `Box` for a request.

## Tests

- An unpinned page makes room; a pinned page is never the victim.
- A dirty page survives eviction; a clean page costs no write; dirt accumulates across pins.
- Many pages pass through a small pool and all read back.
- For random sequences, the pool matches the model (the invariant above).
- The pool works under the ARC and LRU-K replacers you built.

## Hints

### Write the lifecycle of a frame

Free; holds a page, pinned; holds a page, unpinned, clean; holds a page, unpinned, dirty; being evicted. For each arrow, say what changes in the page table, the free list, the metadata and the replacer.

### The replacer and the pins

The replacer must be told the frame is *not* evictable while pinned, and evictable when the pin count reaches zero. Which two functions of the pool do that? What goes wrong if you forget either?

### The write comes before the reuse

If you read the new page into the frame before writing the old one out, the old bytes are gone. Sketch the order of the steps and the lock around them.

## Performance

An eviction with a dirty victim costs a write (microseconds) plus a read. A pool whose victims are mostly clean does half the I/O of one whose victims are dirty, which is why databases run a background writer. The replacer keeps hot pages in memory; here is where you can see it: the hit rate decides the speed of everything above.

**Measure it.** Run 100 000 random fetches over 1 000 pages through a pool of 100 frames with the FIFO replacer of the test file and with your ARC replacer, and count the disk reads. Predict the ratio for a uniform workload and for a skewed one.

## Experiment

Optional. Predict first, then run.

1. **Lose the write.** Skip the write-back of dirty victims. Which test fails first, and what does the minimal counterexample (the proptest prints it) look like?
2. **Release the lock during I/O.** Let go of the pool's lock while the victim is written and the new page is read, and run the threaded test of the boss stage. What can two threads now do to each other? (Two fetches of the same page; a fetch of the victim during its write.)

## Other designs

- **Write back on eviction only (ours).** Simplest; evictions may stall on a write.
- **Write-through.** Every unpin(dirty) writes at once; no dirty pages, slow.
- **A background writer.** A thread writes dirty unpinned pages ahead of demand; evictions find clean victims.
- **Pre-evict to keep free frames.** Maintain a small pool of free frames so a fetch never waits for a write.

## In BusTub

BusTub's `CheckedWritePage` and `CheckedReadPage` follow the same steps: look in the page table; else take a free frame; else ask the replacer for a victim, write it back if dirty, and read the new page in. Its tests check the disk's contents after evictions.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `if (frame->is_dirty_) { disk_scheduler_->Schedule(...); promise.get(); }` | a dirty victim is written back (a copy of its bytes, a request, a wait on its future) before the frame is reused |
| `std::optional<frame_id_t> victim = replacer_->Evict(); if (!victim) return std::nullopt;` | `let frame = inner.replacer.evict()?;` |
| `memcpy(frame->data, buf, PAGE_SIZE)` | `frame.copy_from_slice(&buf)` |

**Port rule:** `if (!x) return nullopt;` becomes `x?` when the function returns an `Option`.

## Learn more

- [`Option::take`](https://doc.rust-lang.org/std/option/enum.Option.html#method.take) · [`slice::copy_from_slice`](https://doc.rust-lang.org/std/primitive.slice.html#method.copy_from_slice)
- Hellerstein et al., *Architecture of a Database System*, 4.3 (buffer management)
