Everything an executor needs to find (a table's heap and schema, an index and its key schema) is reached through the **catalog**. It is a registry: tables by **name** (unique) and by **oid** (a number handed out in creation order), indexes by oid and by `(table, name)`. BusTub's catalog is not persistent: it lives in memory for the life of the database instance. `create_index` is the interesting call: it must build the right kind of index for the key's size, choose among the integer-only restrictions, and **fill the new index with the rows the table already has**.

> [!CHECK] `create_index` is called on a table with a million rows, and a row is inserted into the table by another thread while the index is being built. What can the index end up missing, and what would a real database do about it? Which of BusTub's own simplifications make this a non-problem here?
> ||The new index is filled by scanning the table; a row inserted after the scan passed its page is not in the index and nobody will insert it later (the insert executor only updates indexes that existed when it ran). A real database either blocks writers while the index is built (a lock on the table), builds the index while recording concurrent changes in a side log and replays them (online index build), or uses a snapshot. BusTub avoids the problem because DDL runs single-threaded in this course: the catalog takes `&mut self`, so no executor holds the catalog at the time.||
>
> - What does `&mut self` on `create_table` guarantee?
> - Which rows must the new index contain?
> - What happens to rows that share a key?

## The task

`Catalog<'a>` over a buffer pool; tables and indexes are shared as `Arc<TableInfo>` / `Arc<IndexInfo>` (the record types are given).

- `new(bpm)`; `create_table(name, schema) -> Option<Arc<TableInfo>>`: `None` if the name is in use (names are case sensitive); otherwise a new heap and the next **table oid** (0, 1, 2, ...); `get_table(name)`, `get_table_by_oid(oid)`, `table_info(oid)` (borrowed), `get_table_names()`.
- `create_index(name, table, key_attrs, is_primary_key) -> Result<Option<Arc<IndexInfo>>>`: `Ok(None)` if the table does not exist or has an index of that name; an **error** unless every key column is an `INTEGER` (and the key is 1 to 64 bytes, 4 per column); build an index of the smallest key size that fits (4, 8, 16, 32 or 64 bytes); insert the key of **every live tuple** of the table (deleted ones are not indexed; of several rows with the same key the first is kept); the next **index oid**.
- `get_index(name, table)`, `get_index_by_oid(oid)`, `get_table_indexes(table)` (in creation order).

The tests: a created table is found by name and oid; oids in creation order; every table has its own heap; names are unique and case sensitive; many tables; a new index holds the table's existing rows; deleted rows and duplicate keys; the failures that are `Ok(None)` and the ones that are errors; composite indexes; and a property: random table names (with repeats) behave like a `HashMap` (oids, lookups, listing) and an index created over a table with random live and deleted rows holds exactly the first live row of each key, in key order. The boss test uses the catalog, a heap and an index together.

## Your freedom

How you store the registry (maps keyed by oid and by name, a `Vec` indexed by oid, a nested map for the indexes of a table), and how `create_index` chooses the key size.

## The Rust toolbox

**Several maps for several lookups.** `HashMap<TableOid, Arc<TableInfo>>` plus `HashMap<String, TableOid>`: the name map holds the oid, the oid map holds the object, so one object is stored once and `Arc` clones are what you hand out.

**`Arc` for shared ownership of table info.** `Arc::clone(&info)` is cheap; executors keep their own `Arc<TableInfo>` for the life of a query while the catalog keeps its own.

**`&mut self` for DDL.** `create_table` and `create_index` take `&mut self` so no one else can look at the catalog while it changes; lookups take `&self`.

**`Result<Option<T>>`.** `Err` for a real error, `Ok(None)` for "nothing to do or not found", `Ok(Some(x))` for success: a `match` on the three cases is clear.

**`match` on a range.** `match key_size { 1..=4 => .., 5..=8 => .., _ => return Err(..) }` chooses the key size.

**Boxing different types behind one trait.** `let index: Box<dyn Index + 'a> = match .. { 4 => Box::new(BPlusTreeIndex::<4>::new(..)), 8 => Box::new(BPlusTreeIndex::<8>::new(..)), .. };` each arm has a different concrete type; the annotation tells the compiler which common type they convert to.

## If this is new

- [S4 Maps & sets](/t/s4-maps-sets): `HashMap`, `entry`.
- S7 Smart pointers & interior mutability: `Arc`.
- [L4 Traits & dispatch](/t/l4-traits-dispatch): `Box<dyn Trait>` with different concrete types.
- The optional *the catalog* concept.
- Y5 Testing & verification: Understand it: a heap checked against a `Vec`; the Halloween problem as a property.

## Tests

- Tables are found by name and oid; oids count from zero in creation order; names are unique and case sensitive.
- An index is filled with the table's rows; deleted rows and duplicate keys; failures as `Ok(None)` and errors; found by name, oid and table.
- Properties against maps; the boss ties heap, index and catalog together.

## Hints

### The order of creation

`create_index` needs the table's heap: it walks `make_iterator()` and inserts every live tuple. Do it before you register the index, so a failure leaves nothing half made.

### Keys are tuples

The key tuple of a row is `tuple.key_from_tuple(&table.schema, &key_schema, &key_attrs)`: module 3b did the byte work.

### Where do oids live?

A counter in the catalog; increment after you succeed, not before: a failed `create_table` must not consume an oid.

## Performance

The catalog is read once per query (to bind names) and written rarely: a `HashMap` per lookup is plenty. The cost of building an index is a scan plus an insert per row: `O(n log n)` with page I/O; real systems sort the keys first and bulk-load the tree.

**Measure it.** Create an index on a table of 100 000 rows and time it; predict how much of it is the scan and how much the tree inserts.

## Experiment

Optional. Predict first, then run.

1. **Make it persistent.** Store the table names and header page ids in a catalog page. Which of the two registries (names, oids) is needed on disk?
2. **A rollback.** Make `create_index` undo itself on error. Where in your code is the first point that has already changed the catalog?

## Other designs

- **In-memory maps (ours, BusTub's).**
- **A catalog stored in tables** (PostgreSQL's `pg_class`): queries over the catalog itself.
- **A versioned catalog** for transactional DDL.
- **Names resolved once** into plan nodes (module 3d) so the catalog is not consulted per row.

## In BusTub

`TableHeap`, `TableIterator`, `Catalog`, `IndexInfo` and `TableInfo` are the pieces the execution engine of Project 3 uses: executors never touch pages, only the catalog and the table heap.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::unordered_map<table_oid_t, std::unique_ptr<TableInfo>>` | `HashMap<TableOid, Arc<TableInfo>>` |
| `auto CreateTable(...) -> TableInfo *` (null for a duplicate) | `Option<Arc<TableInfo>>` |
| `throw NotImplementedException` | `Err(Exception::new(ExceptionType::NotImplemented, ..))` |
| `std::unique_ptr<Index>` chosen by a `switch` on key size | `Box<dyn Index>` chosen by a `match` |

**Port rule:** a raw pointer that may be null becomes an `Option`; a `unique_ptr` to a base class becomes a `Box<dyn Trait>`.

## Learn more

- [`HashMap`](https://doc.rust-lang.org/std/collections/struct.HashMap.html) · PostgreSQL's [system catalogs](https://www.postgresql.org/docs/current/catalogs.html)
