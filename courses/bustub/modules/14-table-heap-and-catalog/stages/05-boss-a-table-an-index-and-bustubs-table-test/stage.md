**Where this fits.** The end of the storage layer for Project 3: a table, an index and the catalog used together, and BusTub's own `TableHeapTest`.

> [!CHECK] The index is updated by the insert and delete code, not by the table heap. Name the invariant that must hold between a table and each of its indexes after every statement, and say which module's code is responsible for keeping it.
> ||Every live row with a non-NULL key has exactly one entry in each index of its table (its key mapped to its record id), a row whose key has a NULL has none, and every entry names a live row with that key. Keys are unique, so a statement that would give two live rows one key must fail, not leave one of them out. The heap does not know indexes exist, so the **executors** (module 3e: insert, update, delete) must update the table and every index of it together; a bug in one of them is exactly an index that disagrees with its table, which a test of this stage can detect by comparing a scan of the table with a scan of the index.||
>
> - Who updates the index when a row is deleted?
> - What does the index do for a deleted row that is still in the heap?
> - What breaks if an update changes a key column?

## The task

Nothing new to design.

- **A table and its index through the catalog.** Create a table and a primary-key index, insert 400 rows (and their index entries), delete every third key (mark the tuples deleted and delete the index entries); the index lists exactly the live rows in key order, a scan of the table finds the same rows, and every record id in the index reads back its row.
- **BusTub's `TableHeapTest`** (`tuple_test.rs`): 5 000 inserts of the same tuple into a table in a 50-frame pool, a walk of the table with an iterator, and a catalog-and-index round trip.

## Your freedom

None new.

## The Rust toolbox

**Compare two views of one table.** A scan of the heap and a scan of an index must agree; a test that computes both and compares is the cheapest consistency check you can write for your own executors.

## If this is new

- Everything is in the earlier stages of this module.

## Tests

- A table and its index agree after inserts and deletes through the catalog.
- BusTub's `TableHeapTest` and the catalog round trip.

## Hints

### The index disagrees with the table

Print both scans side by side. A key in the index that is not in the table means a delete forgot the index; the other way round, an insert did.

## Performance

5 000 inserts take a few milliseconds in release mode; in debug the page latches and bounds checks dominate. Run `cargo test --release` when you want timings.

## Experiment

Optional. Predict first, then run.

1. **Many indexes.** Create three indexes on one table and measure the cost per insert. How does it grow?
2. **A bigger pool.** Run the table test with 5 frames and with 500: what does the miss count say about your heap's access pattern?

## Other designs

None for this stage. The *Other designs* sections of 3c-01 to 3c-04 list the alternatives to compare with yours.

## In BusTub

`TableHeapTest` is the project's first integration test of pages, heap and iterator: it inserts 5 000 tuples built from a schema and scans them back.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `TEST(TableHeapTest, TableHeapTest)` | `#[test] fn table_heap_test()` |
| `ConstructTuple(&schema)` | the same helper, generating a value per column type |

**Port rule:** the test's file-backed disk is an in-memory disk in the port.

## Learn more

- BusTub's [table heap test](https://github.com/cmu-db/bustub/blob/master/test/table/tuple_test.cpp)
