---
title: Unique indexes under MVCC: tombstones and reusing a rid
summary: Why an index entry cannot be removed when a tuple is deleted, how the index becomes the arbiter of a primary key, and how inserting a key whose tuple is a tombstone reuses the rid instead of making a second tuple.
minutes: 12
---
A table `users` has a primary key `id`, and the primary-key index maps each id to the rid of its tuple. Three transactions touch the user with id 2. Transaction T1 deletes the row and commits at timestamp 6. Transaction T2 started at timestamp 4 and is still running: it asked for a snapshot of the world at 4, and at 4 user 2 existed. Transaction T3 starts at timestamp 9 and inserts a user with id 2 again, a different person with the same number.

Everything about this module's rules follows from asking what each of the three must be able to do.

## Why a delete cannot touch the index

When T1 deleted the row, the obvious tidy-up was to remove the index entry for key 2: the key no longer exists, so why keep a path to it? But T2 must still read user 2. It looks up key 2 in the index, gets the rid, reads the tuple at that rid, and rebuilds the version it is allowed to see by following the undo logs, which is exactly what the previous article described. If the entry were gone, T2 would find nothing and would wrongly conclude that user 2 never existed.

There is a second reason, which is that T1 might not have committed yet. Until it does, it can still abort, and aborting means restoring the tuple. An entry that was removed for a delete that never happened would have to be put back, and a concurrent insert could have taken its place in between.

So the rule is simple and absolute: **a delete never removes an index entry.** Deleting a tuple sets `is_deleted = true` on the tuple's metadata and leaves an undo log, and that is all. The row is now a **tombstone**: still in the table, still reachable through the index, but invisible to any transaction whose snapshot is after the delete. A full system removes the entry much later, once no reader can possibly need it; this course never does.

## The index decides; visibility is the reader's business

Two different questions get mixed up here. "Does this key exist in the table?" and "Can *I* see a row with this key?" have different answers for different transactions, and the index can only answer a crude version of the first. The index says "key 2 is at rid 7". It says nothing about timestamps. A reader at timestamp 4 gets rid 7 and reconstructs a row. A reader at timestamp 7 gets rid 7, finds the tombstone, and sees no tuple. Both are right. In a primary-key index neither needs to re-check that the row's key equals the key it searched for: a rid belongs to one key for its entire life, so that check would never fail. An index on another column is different: an update in place can change that column, the row keeps its rid, and the index ends up with an entry for the old key and one for the new ([index entries](index-entries-key-and-rid)). There the reader must re-check the version it reads against the entry's key.

That crude answer is still exactly what a unique constraint needs, because uniqueness is about the future as well as the present. The index is the one place where two transactions that both want key 2 meet.

## Inserting a key: three cases

Look the key up first, then decide.

**No entry.** The key has never been used. Insert the tuple into the table first, with the transaction's temporary timestamp, because the index entry needs a rid to point to, and then insert the entry `key → rid`. There is a race here: two transactions can both find no entry and both insert a tuple. The entry insert is the arbiter: the first one succeeds, and the second one's `insert_entry` fails. That second transaction now owns a tuple that nobody will ever point to. It must **bury** it (mark it deleted so no scan returns it) and fail with a write-write conflict.

**An entry, and the tuple at its rid is live.** The key is in use by a row that exists. This is a duplicate-key error. The transaction is tainted, and the statement fails. No tuple is created.

**An entry, and the tuple at its rid is a tombstone.** This is T3's case. The key's row was deleted, so the key is free to use, but its index entry is still there and still points at rid 7. Making a second tuple for key 2 would give the key two rids, and the index can store only one. So T3 **reuses the rid**: it brings the tombstone back to life with its new values, and it does so as an ordinary in-place change. That means everything an update does: a write-write conflict check, an undo log that says "the previous version of this tuple was a deleted one, as of the tombstone's timestamp", and the chain's head link moved onto that log. The figure shows the result. The rid and the index entry never change; the chain says who sees what.

```svg
caption: Key 2 across three transactions. The rid never changes; the chain says who sees what.
<svg viewBox="0 0 760 190" role="img" aria-label="One rid with a live version, a tombstone and a reinsert">
<defs><marker id="ui-a" markerWidth="9" markerHeight="9" refX="7" refY="4.5" orient="auto"><path d="M0 0 L9 4.5 L0 9 z" style="fill:var(--dim)"/></marker></defs>
<rect class="hot" x="20" y="40" width="200" height="60" rx="3"/><text class="mid fg" x="120" y="66">table: (2, 7) ts=9</text><text class="mid dim sm" x="120" y="86">live again (txn 9 reinserted)</text>
<rect class="blue" x="290" y="40" width="200" height="60" rx="3"/><text class="mid fg" x="390" y="66">log: deleted ts=6</text><text class="mid dim sm" x="390" y="86">"before txn 9: a tombstone"</text>
<rect class="blue" x="560" y="40" width="180" height="60" rx="3"/><text class="mid fg" x="650" y="66">log: (2, 5) ts=2</text><text class="mid dim sm" x="650" y="86">"before the delete at 6"</text>
<path class="ln" d="M222 70 L288 70" marker-end="url(#ui-a)"/><path class="ln" d="M492 70 L558 70" marker-end="url(#ui-a)"/>
<text class="mid fg sm" x="120" y="130">read ts ≥ 9 sees (2, 7)</text><text class="mid fg sm" x="390" y="130">6 ≤ ts &lt; 9: no tuple</text><text class="mid fg sm" x="650" y="130">2 ≤ ts &lt; 6: (2, 5)</text>
<text class="dim sm" x="380" y="170" style="text-anchor:middle">index: 2 → this rid, the whole time</text>
</svg>
```

Read the figure with the three transactions in mind. A reader at timestamp 9 or later sees the new row, `(2, 7)`. A reader whose snapshot is at 6, 7 or 8 sees no tuple, because the delete had happened and the reinsert had not. A reader at 2 to 5 sees `(2, 5)`, the original row; that is T2. One entry in the index serves all of them.

## Updating a key is a delete and an insert

What if an `UPDATE` changes the key itself, as in `SET id = id + 1`? It cannot be an in-place change of one column, because the index is organised by that column. The old key's entry must stay, pointing at a tombstone for the benefit of older readers; the new key needs its own entry, or the tombstone of that key to reuse. So a key update is a delete of the old row followed by an insert of the new one.

The order has a trap in it. Suppose the table holds keys 1, 2, 3 and 4 and the statement is `SET id = id + 1`. If you process row by row, delete 1 then insert 2, the insert finds an entry for key 2 whose tuple is still live (row 2 has not been deleted yet) and reports a duplicate, although the statement as a whole is perfectly valid. The fix is to do **all the deletes first and then all the inserts**: after the deletes, keys 1 to 4 are tombstones, the inserts of 2, 3 and 4 find tombstones and reuse them, and only key 5 needs a new tuple.

## Where it goes wrong

The failures here are quiet. Removing an entry on delete passes every single-transaction test and breaks the first reader that started before the delete. Creating a second tuple for a tombstoned key leaves the index pointing at the old rid, so a later lookup finds a stale tombstone. Burying the loser's tuple is easy to forget, and the leftover shows up as a phantom row in a full scan. Deciding on a duplicate from the index alone, without looking at whether the tuple at the rid is live, turns every re-insert after a delete into an error. And processing a key update row by row produces spurious duplicate errors that depend on the order of the rows, which is how they escape tests that use one row.

## C++ comparison

| C / C++ | Rust |
|---|---|
| `index->ScanKey(key, &result, txn)` and checking `result.empty()` | `index.scan_key(&key)` returns a `Vec<Rid>`; `.first()` is the rid if any |
| `if (!index->InsertEntry(key, rid, txn)) { ... }` | `if !index.insert_entry(&key, rid) { ... }` |
| tombstone reuse via `UpdateTupleInPlace` with a check lambda | `modify_tuple` (the same write path as an update) |

**Port rule:** a unique-index insert is "lookup, then decide among no entry / live / tombstone", and the decision is final only when the index insert succeeds.

## Try it yourself

1. Keys 5, 6 and 7 exist and are live. One transaction runs `SET id = id - 1` over all three. Write the sequence of deletes and inserts, say for each insert which of the three cases it hits, and say how many new tuples are created. What happens with the order "delete 5, insert 4, delete 6, insert 5, delete 7, insert 6"?
2. T1 and T2 both insert key 9, which has no entry. Walk through every interleaving of "insert tuple" and "insert entry" for the two. In which does the loser end up with an orphaned tuple, and what must it do?
3. A reader at timestamp 7 looks up key 2 in the figure and gets the tombstone. What does `reconstruct_tuple` return, and why does the reader not need the index to tell it that the tuple is gone?
4. **Kata (a week from now).** In a blank file, write the `Table` with `insert`, `delete` and the three-way decision from the test below, and make the update-a-key test pass, without looking at it.
5. **Experiment.** Insert 100 000 keys, delete half, then insert 50 000 new keys that were deleted earlier. Count tuples in the table before and after. Predict the numbers, then check that the table did not grow.

## In real code

### Using it: the three cases of an insert

```rust test
use std::collections::BTreeMap;

#[derive(Debug, PartialEq)]
enum Outcome {
    New(usize),
    Reused(usize),
    Duplicate,
}

struct Table {
    rows: Vec<(bool, i32, i32)>, // (deleted, key, value)
    index: BTreeMap<i32, usize>,
}

impl Table {
    fn insert(&mut self, key: i32, value: i32) -> Outcome {
        match self.index.get(&key).copied() {
            Some(rid) if !self.rows[rid].0 => Outcome::Duplicate,
            Some(rid) => {
                self.rows[rid] = (false, key, value); // revive the tombstone
                Outcome::Reused(rid)
            }
            None => {
                self.rows.push((false, key, value));
                let rid = self.rows.len() - 1;
                self.index.insert(key, rid);
                Outcome::New(rid)
            }
        }
    }
    fn delete(&mut self, key: i32) {
        let rid = self.index[&key];
        self.rows[rid].0 = true; // the entry stays
    }
}

#[test]
fn deleting_keeps_the_entry_and_reinserting_reuses_the_rid() {
    let mut t = Table { rows: vec![], index: BTreeMap::new() };
    assert_eq!(t.insert(1, 10), Outcome::New(0));
    assert_eq!(t.insert(1, 11), Outcome::Duplicate);
    t.delete(1);
    assert_eq!(t.insert(1, 12), Outcome::Reused(0));
    assert_eq!(t.rows.len(), 1, "no second tuple was made");
}

#[test]
fn a_primary_key_update_is_all_deletes_then_all_inserts() {
    let mut t = Table { rows: vec![], index: BTreeMap::new() };
    for k in 1..=4 {
        t.insert(k, 0);
    }
    for k in 1..=4 {
        t.delete(k);
    }
    let outcomes: Vec<Outcome> = (2..=5).map(|k| t.insert(k, 0)).collect();
    assert_eq!(outcomes, vec![Outcome::Reused(1), Outcome::Reused(2), Outcome::Reused(3), Outcome::New(4)]);
    assert_eq!(t.rows.iter().filter(|r| !r.0).count(), 4);
}
```

### Using it: a lost race leaves a buried tuple

```rust test
use std::collections::HashMap;
use std::sync::Mutex;

/// `insert_entry` is the arbiter: it returns false for the second inserter of a key.
struct Index(Mutex<HashMap<i32, usize>>);

impl Index {
    fn insert_entry(&self, key: i32, rid: usize) -> bool {
        let mut m = self.0.lock().unwrap();
        if m.contains_key(&key) {
            return false;
        }
        m.insert(key, rid);
        true
    }
}

#[test]
fn the_loser_buries_its_tuple() {
    let index = Index(Mutex::new(HashMap::new()));
    let mut deleted = vec![false, false]; // two tuples, both already in the heap
    assert!(index.insert_entry(7, 0));
    if !index.insert_entry(7, 1) {
        deleted[1] = true; // bury the tuple we made, then fail the statement
    }
    assert_eq!(deleted, vec![false, true]);
    assert_eq!(index.0.lock().unwrap()[&7], 0);
}
```

### In the exercises

- **4b-06:** primary-key inserts: unique check, tombstone reuse, the race.
- **4b-06:** an update of the primary key as deletes followed by inserts.
- **4b-06:** the index scan re-checks an entry of another index against the version it reads.
- **4b-08:** BusTub's `txn_index_test` runs all of it.

### Where it is used

- **PostgreSQL**: a deleted row's index entry is kept until `VACUUM` removes it; a unique check looks at the heap tuple's visibility, and waits for or fails on an in-progress inserter of the same key.
- **MySQL InnoDB**: delete-marked index records stay until purge; a unique check on a delete-marked record is allowed to reuse it.
