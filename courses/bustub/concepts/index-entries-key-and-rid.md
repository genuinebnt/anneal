---
title: "Index entries: a key and a rid"
summary: A B+ tree holds each key once, yet a column like `surname` repeats. Real engines append the row's id to the key, so every entry is unique and the entries of one key sit together in rid order. How a lookup reads them, how a unique index differs, why NULL keys are stored, and what versions do to the entries.
minutes: 8
---
A B+ tree is a sorted map: each key at most once. An index on `orders(customer)` must hold a hundred orders of one customer. The usual answer is not to change the tree but the key: the tree key of an **entry** is the indexed columns **followed by the row's id**, `(key, rid)`. No two rows share a rid, so no two entries collide, and since the key comes first, the entries of one key sit next to each other, ordered by rid.

```svg
caption: The leaves of an index on one column, entries ordered by key and then by rid. A lookup of 7 starts at "7 with the smallest rid" and reads forward while the key is 7; the NULL key sorts first.
<svg viewBox="0 0 760 170" role="img" aria-label="Index entries ordered by key then rid, and a lookup range for key 7">
<defs><marker id="ek-a" markerWidth="9" markerHeight="9" refX="7" refY="4.5" orient="auto"><path d="M0 0 L9 4.5 L0 9 z" style="fill:var(--dim)"/></marker></defs>
<text class="dim sm" x="20" y="24">leaf entries (key, rid), left to right</text>
<rect class="box" x="20" y="40" width="100" height="40" rx="2"/><text class="mid fg sm" x="70" y="65">NULL, (1,2)</text>
<rect class="box" x="125" y="40" width="100" height="40" rx="2"/><text class="mid fg sm" x="175" y="65">3, (0,5)</text>
<rect class="hot" x="230" y="40" width="100" height="40" rx="2"/><text class="mid fg sm" x="280" y="65">7, (0,4)</text>
<rect class="hot" x="335" y="40" width="100" height="40" rx="2"/><text class="mid fg sm" x="385" y="65">7, (0,9)</text>
<rect class="hot" x="440" y="40" width="100" height="40" rx="2"/><text class="mid fg sm" x="490" y="65">7, (2,1)</text>
<rect class="box" x="545" y="40" width="100" height="40" rx="2"/><text class="mid fg sm" x="595" y="65">9, (0,1)</text>
<rect class="box" x="650" y="40" width="90" height="40" rx="2"/><text class="mid fg sm" x="695" y="65">12, (1,0)</text>
<path class="ln" d="M230 120 L545 120" marker-end="url(#ek-a)"/>
<text class="dim sm" x="230" y="145">begin at (7, smallest rid)</text>
<text class="dim sm" x="470" y="145">stop: key 9</text>
</svg>
```

## The three operations

- **Insert** `(key, rid)`: an ordinary tree insert. It fails only if this very entry is already there.
- **Delete** `(key, rid)`: an ordinary tree remove of that entry. The key's other rows keep theirs. A delete therefore needs the row's rid as well as its key, which the executor that deletes the row has.
- **Lookup** `key`: the tree cannot be asked for "7 with any rid", so the lookup asks for the first entry **not less than (7, the smallest possible rid)** and reads forward until the key changes. That is one descent plus a short walk along the leaves: a range scan of width one key.

A full scan of the index gives every row in key order, rows of one key in rid order, which is what `ORDER BY key` needs, ties included.

## A unique index is the same tree with another comparator

A primary key must refuse a second row with a key. Keep the layout and change the **comparator**: compare the key columns only. Two entries with one key are then *equal*, the tree's own refusal of a key it holds becomes the uniqueness check, and it is atomic, which matters when two transactions insert the same key at once. One care point: a delete of `(key, rid)` finds the key's entry whatever the rid, so it must check the stored rid before removing it.

## NULL keys

`WHERE k = NULL` is never true, so a lookup with a NULL returns nothing. But a NULL key is still an entry: if rows with a NULL key were left out, a scan of the index would not list every row, and `ORDER BY k` through the index would lose them. Any fixed place for NULL keeps the order total: PostgreSQL puts NULLs last in an ascending index unless told otherwise; this course puts them first, as its `ORDER BY` does. A primary key refuses NULLs: a NULL identifies nothing.

## Entries and versions

Under MVCC a row has several versions at one rid. If an update changes an indexed column in place, the new version needs the entry `(new key, rid)`, and the old entry `(old key, rid)` must **stay**: a transaction that started earlier still sees the old key and must find the row through it. So the index holds entries that the newest version no longer matches, and a reader must **re-check**: having read the version it may see, it keeps the row only if that version's key is the entry's key. Each row then comes back once, under the key its reader sees. PostgreSQL does the same check (the "recheck" of an index condition against the heap tuple); entries no snapshot can reach any more are garbage that `VACUUM` removes.

## C++ comparison

| C / C++ | Rust |
|---|---|
| `std::multimap<Key, Rid>` (duplicates by design) | `BTreeSet<(Key, Rid)>`: the rid makes every element unique |
| `index.lower_bound({key, kMinRid})` then `while (it->first == key)` | `set.range((key, Rid::MIN)..).take_while(|(k, _)| *k == key)` |
| a `GenericComparator` that ignores the rid for a unique index | `EntryComparator` with the rid left out of the order |

## In real code

### Using it: a non-unique index over a sorted set

```rust test
use std::collections::BTreeSet;

type Rid = (u32, u32); // (page, slot)
const SMALLEST_RID: Rid = (0, 0);

/// An index that is not unique: one entry per row, ordered by key and then rid.
#[derive(Default)]
struct Index {
    entries: BTreeSet<(Option<i32>, Rid)>, // None is NULL, which orders first
}

impl Index {
    fn insert(&mut self, key: Option<i32>, rid: Rid) -> bool {
        self.entries.insert((key, rid))
    }
    fn delete(&mut self, key: Option<i32>, rid: Rid) {
        self.entries.remove(&(key, rid));
    }
    /// The rids of `key`, in rid order; nothing for NULL, which equals nothing.
    fn lookup(&self, key: Option<i32>) -> Vec<Rid> {
        if key.is_none() {
            return vec![];
        }
        self.entries.range((key, SMALLEST_RID)..).take_while(|(k, _)| *k == key).map(|(_, rid)| *rid).collect()
    }
}

#[test]
fn a_key_finds_all_its_rows_in_rid_order() {
    let mut index = Index::default();
    for (key, rid) in [(Some(7), (2, 1)), (Some(3), (0, 5)), (Some(7), (0, 9)), (Some(9), (0, 1)), (Some(7), (0, 4))] {
        assert!(index.insert(key, rid));
    }
    assert!(!index.insert(Some(7), (0, 9)), "the same entry is there once");
    assert_eq!(index.lookup(Some(7)), vec![(0, 4), (0, 9), (2, 1)]);
    index.delete(Some(7), (0, 9));
    assert_eq!(index.lookup(Some(7)), vec![(0, 4), (2, 1)], "deleting one row leaves the key's others");
    assert_eq!(index.lookup(Some(8)), vec![]);
}

#[test]
fn a_null_key_is_stored_first_but_never_found() {
    let mut index = Index::default();
    index.insert(Some(1), (0, 0));
    index.insert(None, (0, 1));
    index.insert(None, (0, 2));
    let order: Vec<Rid> = index.entries.iter().map(|(_, rid)| *rid).collect();
    assert_eq!(order, vec![(0, 1), (0, 2), (0, 0)], "a full scan lists every row, NULL keys first");
    assert_eq!(index.lookup(None), vec![], "k = NULL is never true");
}
```

### Using it: re-checking an entry against the version a reader sees

```rust test
use std::collections::{BTreeSet, HashMap};

/// Each row's key in every version, newest first: (the timestamp the version starts at, its key).
type Versions = HashMap<u32, Vec<(u64, i32)>>;

/// The key of the version of `rid` a reader at `read_ts` sees.
fn visible_key(versions: &Versions, rid: u32, read_ts: u64) -> Option<i32> {
    versions[&rid].iter().find(|(ts, _)| *ts <= read_ts).map(|(_, key)| *key)
}

/// Every row a reader at `read_ts` sees, in key order: an entry counts only if the visible version still has its key.
fn ordered_scan(entries: &BTreeSet<(i32, u32)>, versions: &Versions, read_ts: u64) -> Vec<(i32, u32)> {
    entries.iter().copied().filter(|(key, rid)| visible_key(versions, *rid, read_ts) == Some(*key)).collect()
}

#[test]
fn an_update_adds_an_entry_and_each_reader_finds_the_row_once() {
    // row 1 had the key 2 from timestamp 1 and has had the key 5 since timestamp 10; row 2 has the key 3
    let versions: Versions = HashMap::from([(1, vec![(10, 5), (1, 2)]), (2, vec![(1, 3)])]);
    let entries: BTreeSet<(i32, u32)> = BTreeSet::from([(2, 1), (5, 1), (3, 2)]); // the old entry stays
    assert_eq!(ordered_scan(&entries, &versions, 5), vec![(2, 1), (3, 2)], "an old reader sees row 1 under 2");
    assert_eq!(ordered_scan(&entries, &versions, 12), vec![(3, 2), (5, 1)], "a new reader sees it under 5, once");
}
```

### In the exercises

- **3c-03:** `BPlusTreeIndex` stores the entry `(key, rid)` as the key's bytes followed by the rid's; `EntryComparator` orders by the key columns, then by rid, and leaves the rid out for a primary key; `scan_key` reads from the smallest rid of the key while the key lasts.
- **3c-04:** `create_index` chooses an entry size of the key plus 8 bytes, gives every live row an entry, and fails only for a primary key.
- **3e-02 and 3e-03:** the insert, delete and update executors add and remove entries with the row's rid; only the primary key can refuse a row.
- **3e-04:** a point lookup returns every row of its key; an ordered scan returns rows of one key in rid order.
- **4b-01 and 4b-03:** a transactional insert gives the new row its entries; an update in place adds the new key's entry and keeps the old one for older snapshots.
- **4b-06:** the index scan inside a transaction keeps an entry only if the version it reads still has the entry's key.

### Where it is used

- **PostgreSQL**: since version 12 the heap tuple id is a tie-breaker column of every B-tree key, so duplicates are ordered by tid and a deleted row's entry can be found directly; version 13 adds deduplication into posting lists.
- **InnoDB**: a secondary index entry is the indexed columns followed by the primary-key columns, which makes every entry unique and points to the row.
- **SQLite**: an index record is the indexed columns followed by the rowid.
