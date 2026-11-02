A table on disk is a **heap**: a chain of table pages in the buffer pool with no order, new tuples going wherever there is room at the end. The `TableHeap` is the first object above pages that an executor can use: `insert_tuple` returns a **record id** (`Rid`: page and slot), and `get_tuple(rid)`, `update_tuple_meta(..)` and `update_tuple_in_place(..)` find that tuple again. Record ids never change (slots never move), which is the promise that lets indexes, undo logs and transactions point at tuples.

> [!CHECK] Two threads insert into the same heap at the same moment and the last page has room for exactly one more tuple. What can go wrong without any coordination, and what is the cheapest coordination that prevents it? Why is a per-page latch not enough?
> ||Both read "room for one", both write into the same slot: one tuple overwrites the other, or the page is split into two links of the chain by two threads allocating two next pages. A page latch protects the bytes of one page, but the decision "this page is full, so allocate and link a new one, and make *that* the last page" is a change to the *heap's* state (which page is last) and to the previous page's link; it must be made by one thread at a time. A lock around inserts (`Mutex<last_page_id>`) serialises exactly that.||
>
> - What does the lock protect: bytes or a decision?
> - Which latch order do you take: the heap's lock, then the page?
> - What happens to readers while an insert holds the lock?

## The task

`TableHeap<'a>` over a buffer pool (the pool reference is given). Implement:

- `TableHeap::new(bpm)`: allocate the first page and format it as a table page; remember it.
- `get_first_page_id()`.
- `insert_tuple(meta, tuple) -> Result<Rid>`: store the tuple at the **end** of the table; if the last page has no room, allocate a new page, link it after the last one and store the tuple there. A tuple that does not fit even an empty page is an **error** ("tuple is too large, cannot insert"). Inserts run one at a time.
- `get_tuple(rid) -> Result<(TupleMeta, Tuple)>`, `get_tuple_meta(rid)`, `update_tuple_meta(meta, rid)`, each under the right latch on the rid's page; a bad rid is an error and not a panic.
- `update_tuple_in_place(meta, tuple, rid, check)`: overwrite a tuple of the same length if `check` (an optional closure over the old tuple, run under the page's **write latch**) approves; returns whether it did.

The tests, in addition to exact scenarios (a full page makes the heap start and link a new one; a tuple too big for the rest of a page moves on; threads inserting at once never share a slot; five thousand tuples through a small pool), run a property: random inserts (sizes 1 to 899), deletes, reads and overwrites on a heap against a `Vec` of `(rid, meta, bytes)`: record ids are never handed out twice, every rid reads back what it should, and an overwrite changes only that tuple.

## Your freedom

How the heap remembers the last page (a field under a lock, or by following the chain), when it allocates the next page (before it knows it needs it, or only when full), and how it orders its latches. The table page's layout is yours from 3b.

## The Rust toolbox

**`Mutex<PageId>` for the last page.** `let mut last = self.last_page_id.lock().unwrap();` holds the lock for the rest of the scope; `*last = new_page_id;` updates the value behind the guard. The lock *is* the serialisation of inserts.

**Guards hand over.** `let mut page_guard = self.bpm.write_page(*last); loop { ...; page_guard = next_guard; }` assigns the new page's guard over the old one: the old latch is dropped *after* the new one is held, the crab-walk of module 2.

**`TablePage::new(&mut guard[..])`** gives a view of the page under the guard; the view borrows the guard, so build it inside a block or re-create it each loop turn.

**`Result` with a custom message.** `Err(Exception::new(ExceptionType::Invalid, "tuple is too large, cannot insert"))` when an *empty* page cannot take the tuple: no page ever will.

**A closure parameter.** `check: Option<&dyn Fn(&TupleMeta, &Tuple, Rid) -> bool>`: `check.map_or(true, |c| c(&meta, &tuple, rid))` runs it if present.

**`thread::scope` for a stress test.** Many threads borrow the heap (`&heap`) without `Arc`; the scope joins them.

## If this is new

- [C1 Threads & shared state](/t/c1-threads-shared-state): `Mutex`, lock scope, `thread::scope`.
- [L3 Lifetimes](/t/l3-lifetimes): why the heap holds `&'a BufferPoolManager`.
- [S1 Option & Result](/t/s1-option-result): `Result`, `?`, `map_or`.
- The optional *heap files* and *coarse and fine-grained locking* concepts.
- Y5 Testing & verification: Understand it: a heap checked against a `Vec`; the Halloween problem as a property.

## Tests

- A new heap has an empty first page; inserts get consecutive slots; a full page makes the heap start and link a new one; a tuple too big for the rest of a page moves on; one that fits no page is an error.
- Metadata is stored with the tuple; 5 000 tuples go through a small pool; threads inserting never share a slot.
- Gets and updates by rid, bad rids are errors, a delete changes only the meta, an in-place update runs its check under the latch and keeps the length.
- Property: the heap behaves like a `Vec` of tuples.

## Hints

### Where does the lock go?

Take the heap's lock first, then the last page's write latch. Anyone else who wants the last page for writing must hold the lock too (insert is the only one that writes the last page's tail).

### The loop

"While the tuple does not fit: allocate, link, move on" is a loop; an empty page that cannot take the tuple ends it with an error. Draw three pages and a tuple that does not fit the first two.

### Dropped guards

If a test says the pool ran out of frames, a guard is being kept: the old page's guard must be dropped when you move on to the next.

## Performance

An insert is the lock, a latch, a size check, a slot write and a copy: about a microsecond with the page in memory. The lock serialises all inserts: a million inserts per second is the ceiling of this design; real engines keep a free-space map and insert into several pages in parallel.

**Measure it.** Insert a million 100-byte tuples from 1, 2 and 4 threads: predict the scaling (it will not scale).

## Experiment

Optional. Predict first, then run.

1. **No lock.** Remove the lock and run the threaded test: how soon does it fail?
2. **A free-space hint.** Keep a list of pages with room, so a delete-heavy table reuses space. What breaks about "inserts go at the end"?

## Other designs

- **A heap with a lock on the last page (ours).**
- **A free-space map** (PostgreSQL): inserts go to any page with room; concurrent inserters rarely collide.
- **Append-only log-structured tables:** no in-place anything; compaction later.
- **Clustered tables** (a B+ tree holds the rows): no heap at all (InnoDB).

## In BusTub

`TableHeap`, `TableIterator`, `Catalog`, `IndexInfo` and `TableInfo` are the pieces the execution engine of Project 3 uses: executors never touch pages, only the catalog and the table heap.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::mutex latch_` and `std::scoped_lock` | `Mutex<PageId>` and its guard |
| `page_guard.AsMut<TablePage>()` | `TablePage::new(&mut guard[..])` |
| `std::function<bool(const TupleMeta &, const Tuple &, RID)> check` | `Option<&dyn Fn(&TupleMeta, &Tuple, Rid) -> bool>` |
| `throw ExecutionException("tuple is too large")` | `Err(Exception::new(..))` |

**Port rule:** a lock member plus a scoped lock becomes a `Mutex` whose guard is the scope.

## Learn more

- [`Mutex`](https://doc.rust-lang.org/std/sync/struct.Mutex.html) · PostgreSQL's [free space map](https://www.postgresql.org/docs/current/storage-fsm.html)
