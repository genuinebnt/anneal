Recovery will apply log records to pages, some of which may already hold the change. A record that says "add 5 to this slot" is dangerous: applied twice it adds 10. A record that says "this slot goes from *before* to *after*" is safe, **if applying it means putting the slot in the `after` state, whatever it holds now**. That makes redo **idempotent**, the property ARIES gets from page LSNs and this course gets from the form of its records. This stage builds the one primitive that every later stage stands on: `set_state(rid, state)`, which puts a heap slot in a given state, `Some(bytes)` for a live record and `None` for no live record, creating the slot if it is the next one, and the `Store` that owns the pages it works on.

**The store.** The module's tests work on a small transactional **store** (`src/recovery/store.rs`): a few heap pages in the buffer pool holding fixed-size records of 16 bytes, each addressed by a `Rid`. Transactions insert, update and delete records; `Store::get` and `scan` read them. Changes are applied to the pages at once (no isolation: that was module 4a and 4b), and a record written by an active transaction cannot be written by another until the first one ends.

> [!CHECK] A page has three slots. `set_state(slot 1, Some(x))` is called, then again with the same arguments. Then `set_state(slot 5, Some(y))`. Then `set_state(slot 3, None)`. What happens in each case and why? Why is "create the slot only if it is the next one" a safe rule for redo, given that records are applied in log order?
> ||The first call overwrites slot 1; the second leaves the page as it was (same state). Slot 5 is an error: slots can only be created right after the last one (slot 3), because the heap page assigns slot numbers in order and has no way to leave a gap. Slot 3 with `None` is an error too: there is nothing to delete yet. In redo, records are applied in the order they were logged and a record that creates slot n is logged after the one that created n-1, so by the time redo reaches it the page has n slots already, or the page on disk is older and has exactly n-1 once the earlier records have been replayed.||
>
> - What is the state of a deleted slot, and how does it differ from a slot that never existed?
> - What does bringing back a deleted slot do to its bytes?
> - Which latch do you hold while you look at the slot count and while you write?

## The task

In `src/recovery/store.rs`:

- `Store::open(bpm, log, pages, first_txn)` (and `pages()`): a store over these heap pages (already formatted: `Store::create`, given, formats new ones and writes them to disk). Keep the pool, the log and the page ids; `first_txn` numbers the transactions (stage 4 uses it).
- `Store::set_state(&self, rid, state: &Option<Vec<u8>>) -> StoreResult<()>`: under the page's **write latch**: `Some(bytes)`: a live record with those bytes: overwrite the slot if it exists (and bring it back to life if it was deleted), create it if it is the next one; `None`: mark an existing slot deleted. Everything else is an error and changes nothing: a record that is not `RECORD_LEN` (16) bytes, a slot beyond the next one, a delete of a slot that does not exist.

(`get` and `scan`, given, read live records back; the store is built on the table pages of module 3b: `TablePage::insert_tuple`, `update_tuple_in_place_unsafe`, `update_tuple_meta`.)

The tests: exact scenarios (the next slot is created and an existing one overwritten; `None` marks a slot deleted and `Some` brings it back; doing it twice is the same as once; a slot cannot be skipped and a missing one cannot be deleted; the length of a record is fixed; pages are independent and the state survives a page write and a restart), and a property: **a random sequence of `set_state` calls on one page against a `Vec<Option<record>>`**: the page does what the vector does and refuses what the vector cannot.

## Your freedom

How `Store` keeps its parts (fields are yours), and how you detect "the next slot" (compare with `get_num_tuples`).

## The Rust toolbox

**A guard is a latch.** `let mut guard = self.bpm.write_page(rid.page_id()); let mut page = TablePage::new(&mut guard[..]);` holds the write latch until `guard` drops: everything you do with `page` is atomic with respect to other users of the page.

**`Result` and early exits.** `return Err(refused("..."))` before you change anything keeps "an error changes nothing" true.

**A tuple from bytes.** `Tuple::from_bytes(rid, bytes)` wraps the 16 bytes; the heap page overwrites a tuple only with one of the same length, so the fixed record length is a requirement of the page, not a whim.

**Lifetimes in a struct.** `Store<'a>` borrows the pool and the log for `'a`; the compiler checks that neither is dropped while a store exists.

**Matching on the state.** `match state { Some(bytes) => .., None => .. }` and nested `if slot < slots` keep the four cases (overwrite, create, skip, delete) in one place.

## If this is new

- [L3 Lifetimes](/t/l3-lifetimes): a struct that borrows (`Store<'a>`).
- [S1 Option & Result](/t/s1-option-result): `Option<Vec<u8>>` as a state; early returns with errors.
- [C1 Threads & shared state](/t/c1-threads-shared-state): a guard as a lock scope.
- The optional *ARIES recovery* and *slotted pages* concepts.
- Y5 Testing & verification: Build it: crash a disk at a random point, then recover; a model of the committed state.

## Tests

- Create, overwrite, delete and revive; idempotence; no gaps; no deleting what is not there; fixed length; pages independent; survives a restart.
- Property: the page behaves like a vector of optional records.

## Hints

### Decide before you touch

Compute the four-way case from `slot` against `get_num_tuples()` first. Only then call the page, so an error never leaves a half-done change.

### A deleted slot still exists

`None` does not remove a slot, it flags it: the slot count does not change, and a later `Some` for it overwrites the bytes and clears the flag.

### The slot you create must be the slot you asked for

`insert_tuple` returns the slot it used; if it is not `rid.slot_num()` something is wrong: refuse.

## Performance

`set_state` is one latch, one slot count read and one write: well under a microsecond with the page in memory. Redo of a million records is a million such calls, which is why real systems skip records already on the page (the page LSN) and prefetch pages.

**Measure it.** Apply a million `set_state` calls to 8 pages in order; compare with the same calls skipping those whose `after` already equals the page's bytes.

## Experiment

Optional. Predict first, then run.

1. **An operation instead of a state.** Make an update "add one" instead of "set". Which test shows what a repeated redo would do?
2. **Allow gaps.** Let `set_state` create slot 5 on an empty page by filling the gap with deleted slots. Which test fails, and what would redo need to guarantee for that to be safe?

## Other designs

- **Records are states (ours):** idempotent by construction.
- **Page LSNs (ARIES):** each page stores the LSN of its last change; redo skips records at or below it.
- **Physiological logging:** the record names a page and an operation on it ("insert tuple at slot 3"), and the page LSN guards against repeats.
- **Logical logging:** the record is the SQL-level operation (needs the index and the heap to be consistent to redo).

## In BusTub

BusTub's 2025 projects stop before recovery: Project 4 is concurrency control. The lectures on logging and recovery (CMU 15-445) teach the write-ahead log and ARIES, and BusTub's older years had a `LogManager` and `LogRecovery` that students filled in. This module is new in this course: a small store that logs before it writes, and recovery you can crash at every step.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `page->InsertTuple(meta, tuple, &rid)` returning a bool | `page.insert_tuple(&meta, &tuple)` returning `Option<u16>` |
| `WritePageGuard guard = bpm->WritePage(id); auto *page = guard.AsMut<TablePage>();` | `let mut guard = bpm.write_page(id); TablePage::new(&mut guard[..])` |
| return codes for failures | `Result` with an `Exception` |

**Port rule:** a guard object that releases the latch in its destructor is a guard value that does the same on drop.

## Learn more

- [`TablePage` of module 3b](https://github.com/cmu-db/bustub/blob/master/src/storage/page/table_page.cpp) · *Idempotent operations* in any distributed-systems text
