A sequential scan reads every tuple of a table. The `TableIterator` is a cursor over the heap: it remembers a record id, moves to the next slot, then to slot 0 of the next page of the chain, and ends after the last. Two details make it a database iterator and not just a loop. It returns **deleted tuples too**, with their metadata (deciding what a query sees is the job of the layer above, and an MVCC reader may want the old version). And it can be made to **stop where the table ended when it was created**: without that, the statement `INSERT INTO t SELECT * FROM t` would read the rows it is itself inserting and never stop. That is the **Halloween problem**, named after the day it was found.

> [!CHECK] `UPDATE salaries SET pay = pay * 1.1 WHERE pay < 50000` is executed by scanning the table with an index on `pay` and updating each row in place. What goes wrong, and which of the two iterators (snapshot at creation, or eager) does a sequential scan use for an `INSERT .. SELECT` from the same table?
> ||The update changes `pay`, which moves the row later in the index's scan order, so the scan finds the same row again and applies the raise repeatedly until it is no longer under 50 000: every row ends up with a different, wrong value. For a heap scan the equivalent is a statement that inserts into the table it reads: it must not see its own output. The **snapshot** iterator (it stops at the last tuple that existed when it was created) is the one `INSERT .. SELECT` uses; the eager one, which continues to the live end, is for scans that want to see concurrent inserts.||
>
> - What does the snapshot iterator remember, besides its position?
> - What if the stop position is exactly the start of a new page?
> - Why does the iterator return deleted tuples?

## The task

- `heap.make_iterator()`: a cursor at the first tuple that stops at the end of the table **as it is now** (remember the last page and how many tuples it has). `heap.make_eager_iterator()`: a cursor at the first tuple with no stopping point.
- `TableIterator`: `get_tuple()` (the tuple and its metadata at the cursor, an error at the end), `get_rid()`, `is_end()`, `advance()` (the next slot, or slot 0 of the next page, or the end; the end also when the stopping record id is reached) and `Iterator` (`for (meta, tuple) in heap.make_iterator()`).
- An empty table, or a start that names no tuple, is at the end.
- Every tuple is returned, deleted ones included.

The tests: exact scenarios (an empty table; every tuple in insertion order across pages; a cursor driven by hand; deleted tuples with their meta; the snapshot iterator stops where the table ended; the stop position can be a page boundary), and a property: random tables (inserts of sizes 1 to 699 and deletes) are read by a snapshot iterator made *before* more inserts and an eager one: the snapshot returns exactly the first table, the eager one all.

## Your freedom

What the iterator holds (a rid and a stop rid, or the page id and a slot, or a copy of the current page's tuples), how it finds the end, and how it avoids holding a latch between calls.

## The Rust toolbox

**`impl Iterator` for a cursor type.** `type Item = (TupleMeta, Tuple); fn next(&mut self) -> Option<..>`: return `None` at the end, otherwise the current tuple after moving on. `for (meta, tuple) in heap.make_iterator()` then works, and so does `.filter`, `.map`, `.count`.

**A lifetime for the heap borrow.** `TableIterator<'a>` holds `&'a TableHeap<'a>`; the compiler guarantees the heap outlives every iterator. If you see "cannot borrow ... as mutable because it is also borrowed as immutable", a live iterator is being used while the table is modified: `insert_tuple` takes `&self`, so it works, which is the design.

**`Rid::default()` is an invalid id.** Use it as "no stop position" for the eager iterator; compare with `==`.

**Read a page briefly.** `let guard = heap.bpm.read_page(id); let page = TablePage::new(&guard[..]); let n = page.get_num_tuples();` and let the guard drop before returning from `advance`.

**A snapshot of the end.** `let last = *self.last_page_id.lock().unwrap(); let n = TablePage::new(&bpm.read_page(last)[..]).get_num_tuples();` the pair `(last, n)` is the stopping rid.

## If this is new

- S6 Iterators: implementing the trait, `for` desugaring.
- [L3 Lifetimes](/t/l3-lifetimes): a struct that holds a reference.
- The optional *halloween problem* concept explains the bug with an example.
- Y5 Testing & verification: Understand it: a heap checked against a `Vec`; the Halloween problem as a property.

## Tests

- An empty table has nothing to iterate; a scan visits every tuple in insertion order across pages; a cursor can be driven by hand; deleted tuples come with their meta.
- An iterator stops where the table ended when it was made; the stop can be a page boundary; the eager iterator has none.
- Property: snapshot and eager iterators over random tables.

## Hints

### The stop position at a page boundary

If the table had exactly a full last page when the iterator was made, the stop position is `(last page, number of tuples)`: after the next advance the cursor is at that rid and must end, even though a *next* page now exists.

### Do not skip deleted tuples

Resist the temptation to be helpful: the sequential-scan executor skips them, and MVCC (module 4) needs to see them.

## Performance

A scan latches each page once per tuple in this simple design; a faster scan copies a page's tuples (or keeps its latch) and serves several from one latch, which is why real executors scan page by page.

**Measure it.** Scan a table of a million small tuples with this iterator and with direct page reads; predict the latch overhead per tuple.

## Experiment

Optional. Predict first, then run.

1. **Insert while scanning.** With the eager iterator, insert one tuple per tuple read from a one-page table. When does it stop, and what does that tell you about `INSERT .. SELECT`?
2. **Cache the page.** Keep the current page's tuples in a `Vec` inside the iterator. How much faster is a scan, and what stale data can it show?

## Other designs

- **A rid cursor with re-latching (ours).**
- **A page-at-a-time cursor** (holds one page's tuples).
- **A cursor that holds the page latch** between calls (blocks writers; used for short scans).
- **A materialised snapshot** (copy the whole table): the simplest way to solve the Halloween problem, at memory cost.

## In BusTub

`TableHeap`, `TableIterator`, `Catalog`, `IndexInfo` and `TableInfo` are the pieces the execution engine of Project 3 uses: executors never touch pages, only the catalog and the table heap.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `class TableIterator { RID rid_; RID stop_at_rid_; }` | `struct TableIterator<'a>` with the same two rids |
| `operator++` / `operator*` / `IsEnd()` | `advance`, `get_tuple`, `is_end` and the `Iterator` trait |
| `MakeIterator()` / `MakeEagerIterator()` | the same two methods |

**Port rule:** a C++ iterator with `*` and `++` becomes a Rust `Iterator`, and the explicit `IsEnd()` stays for the tests written the C++ way.

## Learn more

- [`Iterator`](https://doc.rust-lang.org/std/iter/trait.Iterator.html) · The Halloween problem: <https://en.wikipedia.org/wiki/Halloween_Problem>
