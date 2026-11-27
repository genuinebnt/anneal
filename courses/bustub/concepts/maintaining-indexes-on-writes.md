---
title: Maintaining indexes on writes
summary: An index is derived data: every insert, delete and update must change the table and each index consistently. How a key is built from a row, what to do with duplicates, why an update is delete plus insert, and what goes wrong in each order.
minutes: 9
---
A table of a million rows with an index on `email` lets `where email = 'a@b.c'` find its row in a few page reads instead of a million. The price: the index is a **second copy** of part of the table, and it is only correct if every statement that changes the table also changes it. The index never changes by itself.

```svg
caption: One row, two structures. The table stores whole rows at rids; the index stores (key → rid). An insert writes both; a delete removes both entries; an update to an indexed column moves the index entry from the old key to the new one.
<svg viewBox="0 0 760 200" role="img" aria-label="A table with rows and an index pointing to them">
<defs><marker id="mi-a" markerWidth="9" markerHeight="9" refX="7" refY="4.5" orient="auto"><path d="M0 0 L9 4.5 L0 9 z" style="fill:var(--dim)"/></marker></defs>
<text class="dim sm" x="60" y="22">index on a (key → rid)</text>
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
| **insert** a row | store it, get its rid | add `key(row) → rid` |
| **delete** a row | mark it deleted | remove `key(row)` |
| **update** a row | delete the old, insert the new (a new rid) | remove `key(old)`, add `key(new) → new rid` |

A **key** is built from the row by taking the indexed columns: `Tuple::key_from_tuple(table_schema, key_schema, key_attrs)`. Use the *table's* schema to read the row and the *index's key schema* to lay the key out; mixing them gives garbage keys that fail only on indexes that do not start at the first column.

## Duplicates

BusTub's B+ tree stores one entry per key, so `insert_entry` returns `false` for a key that is already there. A **non-unique** index would store `(key, rid)` pairs instead; this engine's indexes are **unique** over non-NULL keys, so the refusal must become an error (`duplicate key value violates unique constraint "<index>"`, PostgreSQL's words) and the statement fails. Ignoring the refusal, as BusTub's C++ executors do, leaves a live row with no entry: a lookup or an `ORDER BY` through the index then silently returns fewer rows than the table holds. The invariant to keep is exact: **every live row with a non-NULL key has exactly one entry in each index of its table**. A key with a NULL has none (NULL equals nothing), so two rows with NULL keys never collide.

## Order matters

- **Update**: remove the old entries *before* adding the new ones, and for a statement that changes many rows, remove **every** old key first. `SET k = k + 1` over the keys 1, 2, 3 moves row 1 onto the old key of row 2: row by row, that insert collides with a key about to leave. Whether the keys are unique is a question about the rows once the statement has run.
- **Delete**: remove the entry using the row's *old* values, which you have only before it is gone.
- **Failures in the middle**: a crash between "wrote the table" and "wrote the index" leaves them inconsistent. Real systems write both inside a transaction with a log (module 5); BusTub's executors do not have to, but know why.

## Why update is delete plus insert

An in-place update is possible only if the new row has the same size and the rid stays valid. A longer string needs a different place, so the general case is a new rid, and then every index entry must be rewritten even for columns that did not change. PostgreSQL's *heap-only tuples* avoid that cost when no indexed column changed; BusTub keeps the simple rule.

## C++ comparison

| C / C++ | Rust |
|---|---|
| `for (auto &index_info : catalog->GetTableIndexes(table_info->name_)) { index_info->index_->InsertEntry(key, rid, txn); }` | `for index in &self.indexes { index.index.insert_entry(&key, rid); }` |
| `tuple.KeyFromTuple(table_info->schema_, index_info->key_schema_, index_info->index_->GetKeyAttrs())` | `tuple.key_from_tuple(&table.schema, &index.key_schema, index.index.metadata().get_key_attrs())` |
| the indexes are looked up in `Next` each time | look them up once in the constructor and keep the `Arc`s |

## In real code

### Using it: a table and one index kept in step

```rust test
use std::collections::BTreeMap;

#[derive(Default)]
struct Table { rows: Vec<Option<(i32, i32)>> } // None = deleted
#[derive(Default)]
struct Index { entries: BTreeMap<i32, usize> }  // key (column a) -> rid
#[derive(Default)]
struct Db { table: Table, index: Index }

impl Db {
    fn insert(&mut self, row: (i32, i32)) -> Result<usize, String> {
        // unique keys: a taken key fails before anything is written
        if self.index.entries.contains_key(&row.0) {
            return Err(format!("duplicate key value violates unique constraint (key {})", row.0));
        }
        self.table.rows.push(Some(row));
        let rid = self.table.rows.len() - 1;
        self.index.entries.insert(row.0, rid);
        Ok(rid)
    }
    fn delete(&mut self, rid: usize) {
        let row = self.table.rows[rid].take().expect("a live row");
        self.index.entries.remove(&row.0);
    }
    fn update(&mut self, rid: usize, new: (i32, i32)) -> Result<usize, String> {
        if self.index.entries.get(&new.0).is_some_and(|holder| *holder != rid) {
            return Err(format!("duplicate key value violates unique constraint (key {})", new.0));
        }
        self.delete(rid);          // the old entry goes first ...
        self.insert(new)           // ... then the new one, under the new rid
    }
    fn lookup(&self, key: i32) -> Option<(i32, i32)> {
        self.index.entries.get(&key).and_then(|rid| self.table.rows[*rid])
    }
}

#[test]
fn insert_writes_both_and_the_index_leads_to_the_row() {
    let mut db = Db::default();
    db.insert((1, 10)).unwrap();
    db.insert((2, 20)).unwrap();
    assert_eq!(db.lookup(2), Some((2, 20)));
    assert_eq!(db.lookup(3), None);
}

#[test]
fn delete_removes_the_entry_so_a_lookup_does_not_find_a_dead_row() {
    let mut db = Db::default();
    let rid = db.insert((1, 10)).unwrap();
    db.delete(rid);
    assert_eq!(db.lookup(1), None);
    assert!(db.index.entries.is_empty());
    db.insert((1, 11)).unwrap();
    assert_eq!(db.lookup(1), Some((1, 11)), "the key is free again");
}

#[test]
fn update_moves_the_entry_from_the_old_key_to_the_new_one() {
    let mut db = Db::default();
    let rid = db.insert((2, 20)).unwrap();
    db.update(rid, (8, -20)).unwrap();
    assert_eq!(db.lookup(2), None);
    assert_eq!(db.lookup(8), Some((8, -20)));
    let rid = db.insert((5, 50)).unwrap();
    db.update(rid, (5, 51)).unwrap(); // the key did not change: delete-then-insert keeps the entry
    assert_eq!(db.lookup(5), Some((5, 51)));
    assert_eq!(db.index.entries.len(), 2);
}

#[test]
fn a_duplicate_key_fails_and_changes_nothing() {
    let mut db = Db::default();
    db.insert((1, 100)).unwrap();
    assert!(db.insert((1, 200)).is_err(), "the key 1 is taken");
    assert_eq!(db.table.rows.iter().flatten().count(), 1, "the refused row is not in the table");
    let other = db.insert((2, 300)).unwrap();
    assert!(db.update(other, (1, 300)).is_err(), "an update may not take a key either");
    assert_eq!((db.lookup(1), db.lookup(2)), (Some((1, 100)), Some((2, 300))), "every live row keeps its one entry");
}
```

### In the exercises

- **3e-02:** `insert_into_indexes` builds a key per index and adds it; a key an index already holds fails the insert before the row is written.
- **3e-03:** `delete_from_indexes` removes the entries of a deleted row.
- **3e-03:** the update executor does both, every old key out first, every new key in under its new rid, after checking that the new keys are unique.

### Where it is used

- **PostgreSQL**: `ExecInsertIndexTuples` after the heap insert; deletes leave index entries until `VACUUM`; HOT updates skip them.
- **InnoDB**: secondary indexes store the primary key; changing an indexed column deletes and inserts the secondary entry (with a change buffer to defer the page reads).
- **SQLite**: the VDBE opcodes `IdxInsert` / `IdxDelete` follow every table write.
- **LSM stores**: a secondary index is another column family written in the same batch.
