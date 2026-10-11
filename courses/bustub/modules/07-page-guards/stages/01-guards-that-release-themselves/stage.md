With `fetch_page` and `unpin_page` every caller must remember to unpin, on every path, including early returns and panics. Forgetting leaks a pin, and a leaked pin is a frame that can never be evicted: a slow, silent way to run the pool out of memory. Rust has a tool for exactly this: **RAII guards**. A guard is a value that holds a pin and a latch, gives access to the bytes while it exists, and releases both when it goes out of scope. This is BusTub's current interface, and the one every higher layer (the hash table, the B+ tree, the table heap) uses.

> [!CHECK] A guard releases two things when it is dropped: the frame's latch and the page's pin. In which order, and what could another thread observe if you swapped them?
> ||Latch first, then unpin. If the pin were released first, the page would become evictable while the guard still holds its latch: another thread could evict the frame and put a different page in it while the old guard's latch is still held, and the latch would then protect the wrong page. Unlatch-then-unpin keeps the invariant "a latched page is always pinned".||
>
> - What state must always hold between a pin and a latch?
> - Where in Rust does destruction order decide behaviour?
> - What does the guard's lifetime parameter say about the pool?

## The task

`checked_read_page(page)` and `checked_write_page(page)` pin the page, take the frame's read or write latch, and return a guard (`None` if the page cannot be brought into memory because every frame is pinned).

- `ReadPageGuard`: `get_page_id()`, `get_data()` (the bytes; panics if the guard was released), `Deref` to the page. Many read guards on one page may exist at once; each holds one pin.
- `WritePageGuard`: the same, plus `get_data_mut()` (marks the page **dirty**) and `DerefMut`, `is_dirty()`. A write guard is alone on its page: it excludes readers and other writers.
- Dropping a guard **unlatches, then unpins**. For a write guard the unpin reports whether `get_data_mut` was used; a guard that was only looked at leaves the page clean (nothing is written when it is evicted).
- `release()` releases early and is **idempotent**: a second call, and the later drop, do nothing.

The tests cover pins and drops, the latch, dirtiness through eviction, and a property: guards taken and dropped in any order on a small pool keep the pin count of each page equal to the number of live guards on it, a writer is alone, and what a writer wrote is what the next guard sees even after the page was evicted in between.

## Your freedom

What a guard stores: the pool and page id; the latch guard itself (`RwLockReadGuard`), or only enough to find the latch again; how `release` is implemented. The guard's fields are private to you.

## The Rust toolbox

**RAII in one line.** `impl Drop for ReadPageGuard<'_> { fn drop(&mut self) { ... } }` runs when the guard goes out of scope: at the end of a block, on `drop(guard)`, on an early `return`, when `?` propagates an error, and when a panic unwinds. No path can forget.

**A lifetime ties the guard to the pool.** `ReadPageGuard<'a>` holds `&'a BufferPoolManager`, so the compiler guarantees the pool outlives every guard. The `'a` also appears in the latch guard `RwLockReadGuard<'a, ..>`, which borrows the frame's `RwLock` from the pool. If the compiler says "borrowed value does not live long enough" about the pool, a guard has escaped the scope of the pool.

**`Option` makes release idempotent.** Store the latch as `Option<RwLockReadGuard<..>>`. `release` does `if let Some(latch) = self.latch.take() { drop(latch); self.bpm.unpin_page(..); }`: the first call takes it, the second finds `None`. `Drop` just calls `release`.

**Dropping in a chosen order.** `drop(latch)` before the unpin call makes the order explicit; relying on field declaration order is fragile. Fields are dropped in the order they are declared, but explicit is clearer.

**`Deref` makes the guard look like the data.** `impl Deref for ReadPageGuard<'_> { type Target = PageData; fn deref(&self) -> &PageData { self.get_data() } }` lets `guard[0]` and `guard.len()` work. `DerefMut` on the write guard calls `get_data_mut`, which marks the page dirty: note that `&mut *guard` anywhere makes the page dirty.

**Compiler messages you will meet.** "cannot move out of `self.latch` which is behind a mutable reference": use `.take()`. "`bpm` does not live long enough": the pool is declared after the guard in the same scope; declare the pool first.

## If this is new

- [L3 Lifetimes](/t/l3-lifetimes): structs that hold references.
- [L2 Borrowing](/t/l2-borrowing) and [L1 Ownership & moves](/t/l1-ownership-moves): why `take()` and `drop()` exist.
- [L4 Traits & dispatch](/t/l4-traits-dispatch): `Deref`, `Drop` are traits.
- The optional concept *RAII guards and lifetimes* is the whole idea with examples.
- [S8 The core traits](/t/s8-core-traits): Implement by hand: `Deref`/`DerefMut`, `Drop` order.
- [S7 Smart pointers & interior mutability](/t/s7-smart-pointers): Understand it: `Deref` coercions, `RwLockReadGuard`.

## Tests

- A read guard pins the page and shows its bytes; many readers share a page; no guard when every frame is pinned.
- Dropping a guard unpins; leaving a scope drops; `release` is idempotent; a released guard gives up the latch; each guard releases only its own pin.
- A write guard changes the page; `get_data_mut` marks it dirty and looking does not; the change survives eviction; an untouched write guard, and a read guard, leave the page clean.
- A writer excludes everyone else until it is dropped.
- For random sequences of guards taken and dropped in any order, pin counts, data and failures match the model.

## Hints

### Pin first or latch first on the way in?

The page must be pinned before the latch is taken (otherwise it could be evicted while you wait), and the pool's lock must not be held while you wait for the latch. Write the three steps of `checked_write_page` in order.

### Who tells the pool the page is dirty?

The guard knows (it handed out `get_data_mut`); the pool needs to know at unpin time. What does `release` pass to `unpin_page`?

### Debugging a pin that never goes away

A page whose pin count never returns to zero has a guard that was leaked: kept in a struct, moved into a thread that never finished, or forgotten with `std::mem::forget`. Print `get_pin_count` at the end of your test to see which page.

## Performance

A guard costs a pin (lock, increment), a latch (an atomic operation when uncontended) and the reverse on drop: roughly 100 ns. Readers do not block each other. A writer waits for all readers to leave; a steady stream of readers can starve a writer, as the latch stage discussed.

**Measure it.** Take a read guard on a resident page one million times from one thread; then from eight threads on the same page; then on eight different pages. The first and third should scale differently from the second: why?

## Experiment

Optional. Predict first, then run.

1. **Wrong order.** Unpin before unlatching and run the thread tests. Does it fail? How often? What does that say about testing a race?
2. **Forget the drop.** Wrap a guard in `std::mem::forget` in a test and look at the pin count. How would you find that leak in a large program?

## Other designs

- **Option of the latch guard (ours).** Simple and idempotent.
- **A flag `released: bool`.** The latch guard is dropped by the struct's field drop; the flag only gates the unpin.
- **Guards as closures** (`bpm.with_read(page, |data| ..)`). Impossible to leak a guard; awkward when a guard must outlive a function (an iterator, a B+ tree traversal).
- **Reference-counted guards.** `Arc` pins that share ownership; heavier, rarely needed.

## In BusTub

```cpp
class ReadPageGuard {
 public:
  auto GetPageId() const -> page_id_t;
  auto GetData() const -> const char *;
  template <class T> auto As() const -> const T *;
  auto IsDirty() const -> bool;
  void Flush();
  void Drop();
  ~ReadPageGuard();
};
```
C++ guards are move-only and a moved-from guard must release nothing; Rust's move semantics give you that without writing a move constructor.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| a destructor `~ReadPageGuard()` plus `Drop()` to release early | `impl Drop` plus `release(&mut self)` |
| move constructor and move assignment that null out the source | automatic: a moved-from value cannot be used |
| `std::shared_lock` member | `RwLockReadGuard` inside the guard |
| `template <class T> As()` casting the bytes | a typed view you write in module 2a |

**Port rule:** a C++ RAII class with a deleted copy constructor and a hand-written move becomes a plain Rust struct with `impl Drop`; the compiler provides the move semantics.

## Learn more

- [`Drop`](https://doc.rust-lang.org/std/ops/trait.Drop.html) · [`Deref`](https://doc.rust-lang.org/std/ops/trait.Deref.html) · [RAII in the Rust reference](https://doc.rust-lang.org/rust-by-example/scope/raii.html)
