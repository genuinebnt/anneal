---
title: Maintaining indexes on writes
summary: An index is derived data: every insert, delete and update must change the table and each index consistently. How a key is built from a row, why an entry carries the rid, where uniqueness is enforced, why an update is delete plus insert, what versions add, and what goes wrong in each order.
minutes: 9
---
A table of a million rows with an index on `email` lets `where email = 'a@b.c'` find its row in a few page reads instead of a million. The price: the index is a **second copy** of part of the table, and it is only correct if every statement that changes the table also changes it. The index never changes by itself.

```svg
caption: One row, two structures. The table stores whole rows at rids; the index stores an entry (key, rid) per row. An insert writes both; a delete removes the row's entry; an update to an indexed column replaces the entry of the old key with one of the new key.
<svg viewBox="0 0 760 200" role="img" aria-label="A table with rows and an index pointing to them">
<defs><marker id="mi-a" markerWidth="9" markerHeight="9" refX="7" refY="4.5" orient="auto"><path d="M0 0 L9 4.5 L0 9 z" style="fill:var(--dim)"/></marker></defs>
<text class="dim sm" x="60" y="22">index on a: entries (key, rid)</text>
<rect class="blue" x="40" y="34" width="190" height="34" rx="2"/><text class="mid t-b sm" x="135" y="56">1  →  (page 0, slot 0)</text>
<rect class="blue" x="40" y="72" width="190" height="34" rx="2"/><text class="mid t-b sm" x="135" y="94">2  →  (page 0, slot 1)</text>
<rect class="blue" x="40" y="110" width="190" height="34" rx="2"/><text class="mid t-b sm" x="135" y="132">3  →  (page 0, slot 2)</text>
<text class="dim sm" x="470" y="22">table (rid → row)</text>
<rect class="live" x="400" y="34" width="280" height="34" rx="2"/><text class="mid fg sm" x="540" y="56">slot 0:   a=1  b=10</text>
<rect class="live" x="400" y="72" width="280" height="34" rx="2"/><text class="mid fg sm" x="540" y="94">slot 1:   a=2  b=20</text>
<rect class="live" x="400" y="110" width="280" height="34" rx="2"/><text class="mid fg sm" x="540" y="132">slot 2:   a=3  b=30</text>
<path class="ln" d="M232 51 C300 51 340 51 398 51" marker-end="url(#mi-a)"/><path class="ln" d="M232 89 C300 89 340 89 398 89" marker-end="url(#mi-a)"/><path class="ln" d="M232 127 C300 127 340 127 398 127" marker-end="url(#mi-a)"/>
<text class="dim sm" x="40" y="180">Every write must touch both. If it forgets the index, a lookup misses the row (or finds a row that is gone).</text>
</svg>
```

## The operations

| statement | table | each index |
|---|---|---|
| **insert** a row | store it, get its rid | add `(key(row), rid)` |
| **delete** a row | mark it deleted | remove `(key(row), rid)` |
| **update** a row | delete the old, insert the new (a new rid) | remove `(key(old), old rid)`, add `(key(new), new rid)` |

A **key** is built from the row by taking the indexed columns: `Tuple::key_from_tuple(table_schema, key_schema, key_attrs)`. Use the *table's* schema to read the row and the *index's key schema* to lay the key out; mixing them gives garbage keys that fail only on indexes that do not start at the first column.

## Duplicates and uniqueness

Rows share keys: many orders have one customer. A tree that holds each key once can still index them, because an entry is `(key, rid)` and no two rows share a rid ([index entries](index-entries-key-and-rid)). So an ordinary index never refuses a row, and a delete must name the entry by its rid as well as its key, or it would remove another row's entry. A key with a NULL is an entry too; a lookup of NULL finds nothing, but a scan of the index lists every row.

**Uniqueness is a constraint**, not a property of every index. A primary key (or a `UNIQUE` index) refuses a second live row with a key, and a NULL key; the refusal must become an error (`duplicate key value violates unique constraint "<index>"`, PostgreSQL's words) and the statement fails. Ignoring the refusal, as BusTub's C++ executors do with their one-key-per-entry indexes, leaves a live row with no entry: a lookup or an `ORDER BY` through the index then silently returns fewer rows than the table holds. The invariant to keep is exact: **every live row has exactly one entry in each index of its table**.

## Order matters

- **Update**: remove the old entries *before* adding the new ones, and for a statement that changes many rows, remove **every** old key first. `SET k = k + 1` over the keys 1, 2, 3 moves row 1 onto the old key of row 2: row by row, that insert collides with a key about to leave. Whether a primary key stays unique is a question about the rows once the statement has run.
- **Delete**: remove the entry using the row's *old* values and its rid, which you have only before it is gone.
- **Failures in the middle**: a crash between "wrote the table" and "wrote the index" leaves them inconsistent. Real systems write both inside a transaction with a log (module 5); BusTub's executors do not have to, but know why.

## Why update is delete plus insert

An in-place update is possible only if the new row has the same size and the rid stays valid. A longer string needs a different place, so the general case is a new rid, and then every index entry must be rewritten even for columns that did not change. PostgreSQL's *heap-only tuples* avoid that cost when no indexed column changed; BusTub keeps the simple rule.

## With versions

Under MVCC (module 4b) a row keeps its rid across versions, and a delete only marks a version: older snapshots must still find the row. So the rules change: a transactional delete removes **no** entry, and an update that changes an indexed column in place **adds** `(new key, rid)` and keeps `(old key, rid)` for the readers that still see the old key. The index then holds entries the newest version does not match, and every reader re-checks: it keeps a row only if the version it may see has the entry's key.

## C++ comparison

| C / C++ | Rust |
|---|---|
| `for (auto &index_info : catalog->GetTableIndexes(table_info->name_)) { index_info->index_->InsertEntry(key, rid, txn); }` | `for index in &self.indexes { index.index.insert_entry(&key, rid); }` |
| `tuple.KeyFromTuple(table_info->schema_, index_info->key_schema_, index_info->index_->GetKeyAttrs())` | `tuple.key_from_tuple(&table.schema, &index.key_schema, index.index.metadata().get_key_attrs())` |
| the indexes are looked up in `Next` each time | look them up once in the constructor and keep the `Arc`s |

## In real code

### Using it: a table and one index kept in step

```rust test
use std::collections::BTreeSet;

#[derive(Default)]
struct Table { rows: Vec<Option<(i32, i32)>> } // None = deleted
#[derive(Default)]
struct Db {
    table: Table,
    index: BTreeSet<(i32, usize)>,   // entries (column a, rid): keys may repeat
    unique: BTreeSet<(i32, usize)>,  // a primary key on column b: one entry per key
}

impl Db {
    fn taken(&self, b: i32, except: Option<usize>) -> bool {
        self.unique.range((b, 0)..=(b, usize::MAX)).any(|(_, rid)| Some(*rid) != except)
    }
    fn insert(&mut self, row: (i32, i32)) -> Result<usize, String> {
        // the primary key is checked before anything is written
        if self.taken(row.1, None) {
            return Err(format!("duplicate key value violates unique constraint (key {})", row.1));
        }
        self.table.rows.push(Some(row));
        let rid = self.table.rows.len() - 1;
        self.index.insert((row.0, rid));
        self.unique.insert((row.1, rid));
        Ok(rid)
    }
    fn delete(&mut self, rid: usize) {
        let row = self.table.rows[rid].take().expect("a live row");
        self.index.remove(&(row.0, rid)); // the entry of this row, not of the key
        self.unique.remove(&(row.1, rid));
    }
    fn update(&mut self, rid: usize, new: (i32, i32)) -> Result<usize, String> {
        if self.taken(new.1, Some(rid)) {
            return Err(format!("duplicate key value violates unique constraint (key {})", new.1));
        }
        self.delete(rid);          // the old entries go first ...
        self.insert(new)           // ... then the new ones, under the new rid
    }
    fn lookup(&self, a: i32) -> Vec<(i32, i32)> {
        self.index.range((a, 0)..=(a, usize::MAX)).filter_map(|(_, rid)| self.table.rows[*rid]).collect()
    }
}

#[test]
fn insert_writes_both_and_a_key_finds_every_row_that_has_it() {
    let mut db = Db::default();
    db.insert((1, 10)).unwrap();
    db.insert((2, 20)).unwrap();
    db.insert((1, 30)).unwrap();
    assert_eq!(db.lookup(1), vec![(1, 10), (1, 30)]);
    assert_eq!(db.lookup(3), vec![]);
}

#[test]
fn delete_removes_only_its_own_row_s_entry() {
    let mut db = Db::default();
    let first = db.insert((1, 10)).unwrap();
    db.insert((1, 11)).unwrap();
    db.delete(first);
    assert_eq!(db.lookup(1), vec![(1, 11)], "the other row with the key 1 keeps its entry");
    assert_eq!(db.index.len(), 1);
}

#[test]
fn update_moves_the_entry_from_the_old_key_to_the_new_one() {
    let mut db = Db::default();
    let rid = db.insert((2, 20)).unwrap();
    db.update(rid, (8, 21)).unwrap();
    assert_eq!(db.lookup(2), vec![]);
    assert_eq!(db.lookup(8), vec![(8, 21)]);
    let rid = db.insert((5, 50)).unwrap();
    db.update(rid, (5, 50)).unwrap(); // nothing changed: delete-then-insert keeps one entry per index
    assert_eq!((db.index.len(), db.unique.len()), (2, 2));
}

#[test]
fn only_the_primary_key_refuses_a_row() {
    let mut db = Db::default();
    db.insert((1, 100)).unwrap();
    assert!(db.insert((1, 200)).is_ok(), "the index on a takes a repeated key");
    assert!(db.insert((7, 100)).is_err(), "the primary key 100 is taken");
    assert_eq!(db.table.rows.iter().flatten().count(), 2, "the refused row is not in the table");
    let other = db.insert((2, 300)).unwrap();
    assert!(db.update(other, (2, 200)).is_err(), "an update may not take a primary key either");
    assert_eq!(db.lookup(2), vec![(2, 300)], "every live row keeps its one entry per index");
}
```

### In the exercises

- **3e-02:** `insert_into_indexes` builds a key per index and adds the entry `(key, rid)`; a primary key that is taken or NULL fails the insert before the row is written.
- **3e-03:** `delete_from_indexes` removes the deleted row's entry, by key and rid.
- **3e-03:** the update executor does both, every old entry out first, every new one in under its new rid, after checking that the new primary keys are unique.
- **4b-01, 4b-03 and 4b-06:** inside a transaction the entries stay: an insert adds the row's entries, an update in place adds the new key's entry, and an index scan re-checks the version it reads.

### Where it is used

- **PostgreSQL**: `ExecInsertIndexTuples` after the heap insert; deletes leave index entries until `VACUUM`; HOT updates skip them.
- **InnoDB**: secondary indexes store the primary key; changing an indexed column deletes and inserts the secondary entry (with a change buffer to defer the page reads).
- **SQLite**: the VDBE opcodes `IdxInsert` / `IdxDelete` follow every table write.
- **LSM stores**: a secondary index is another column family written in the same batch.
