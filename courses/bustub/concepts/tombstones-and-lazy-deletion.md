---
title: Tombstones: deleting by marking, and cleaning up later
summary: What a tombstone is, why databases use them (cheap deletes, immutable storage, concurrency), the rules this leaf buffer follows, and how LSM-trees, PostgreSQL and the B+ tree of this course pay the bill later.
minutes: 10
---
Deleting a key from a sorted array means moving everything after it. Deleting it from a file on an append-only disk means you cannot, since the file never changes. Deleting it while other threads read means someone may still want to see it. All three have the same cheap answer: **do not delete; write down that it is deleted.** The note is a **tombstone**. Reads check for it; a cleanup pass removes the dead data for real, later, when it is cheap or safe.

## The mechanism in a leaf

The leaf keeps a small buffer of tombstoned keys (this course: the last `TOMBS`, oldest first). The pairs stay where they are.

```svg
caption: A leaf with 6 pairs and a tombstone buffer of 2. Deleting 3 and then 4 only writes two small keys; no pair moves. Deleting 5 finds the buffer full: the oldest tombstoned pair (3) is removed for real (the later pairs shift once), and 5 becomes the newest tombstone.
<svg viewBox="0 0 760 290" role="img" aria-label="A leaf page with pairs 1 to 6 and a tombstone buffer, shown after deleting 3, 4 and 5">
<text class="dim sm" x="20" y="22">after delete 3, delete 4</text>
<rect class="box" x="20" y="30" width="120" height="34" rx="4"/><text class="mid dim sm" x="80" y="52">buffer: 3, 4</text>
<g><rect class="live" x="160" y="30" width="70" height="34" rx="4"/><text class="mid fg" x="195" y="52">1</text><rect class="live" x="236" y="30" width="70" height="34" rx="4"/><text class="mid fg" x="271" y="52">2</text>
<rect class="hot" x="312" y="30" width="70" height="34" rx="4"/><text class="mid fg" x="347" y="52">3 ✕</text><rect class="hot" x="388" y="30" width="70" height="34" rx="4"/><text class="mid fg" x="423" y="52">4 ✕</text>
<rect class="live" x="464" y="30" width="70" height="34" rx="4"/><text class="mid fg" x="499" y="52">5</text><rect class="live" x="540" y="30" width="70" height="34" rx="4"/><text class="mid fg" x="575" y="52">6</text></g>
<text class="t-g sm" x="20" y="96">size 6, live 4: nothing moved</text>
<text class="dim sm" x="20" y="140">delete 5: the buffer is full</text>
<path class="ln-w" d="M347 66 V108" /><text class="t-w sm" x="360" y="100">3 is removed for real</text>
<rect class="box" x="20" y="152" width="120" height="34" rx="4"/><text class="mid dim sm" x="80" y="174">buffer: 4, 5</text>
<g><rect class="live" x="160" y="152" width="70" height="34" rx="4"/><text class="mid fg" x="195" y="174">1</text><rect class="live" x="236" y="152" width="70" height="34" rx="4"/><text class="mid fg" x="271" y="174">2</text>
<rect class="hot" x="312" y="152" width="70" height="34" rx="4"/><text class="mid fg" x="347" y="174">4 ✕</text><rect class="hot" x="388" y="152" width="70" height="34" rx="4"/><text class="mid fg" x="423" y="174">5 ✕</text>
<rect class="live" x="464" y="152" width="70" height="34" rx="4"/><text class="mid fg" x="499" y="174">6</text></g>
<text class="t-g sm" x="20" y="218">size 5, live 3</text>
<text class="dim sm" x="20" y="262">insert 4 again: the pair comes back with the new value, no slot used, tombstone dropped</text>
</svg>
```

## The rules (this course's, derived from BusTub's tests)

| operation | rule |
|---|---|
| **delete** key | missing or already tombstoned: nothing. Otherwise: if the buffer is full, really remove the **oldest** tombstoned pair; then buffer the key as the newest |
| **insert** key | physically present and tombstoned: replace the value, drop the tombstone (the pair is back, no new slot). Present and live: refuse. Absent: insert as usual |
| **search**, **scan** | a tombstoned pair is invisible |
| **size** | counts every pair in the page: splits and underflow are about the page, not the live count |
| **split** | each tombstone goes with its pair, order kept |
| **underflow** | the short leaf first really removes its own tombstoned pairs; a deleted pair that is borrowed takes its tombstone along; a merge keeps both buffers, removing the oldest that do not fit |

## Where the bill goes

A tombstone saves work now and creates work later, always:

- **Storage** is held by dead data until it is cleaned up.
- **Reads** must check for tombstones (a scan steps over deleted pairs, so a table that is mostly tombstones scans slowly).
- **A cleanup** (here: the purge when a leaf is short, and the real removal of the oldest when the buffer is full) has to find the dead pairs and shift or rewrite the page.

Whether this is a win depends on the workload: a delete that is soon followed by an insert of the same key (or by nothing) costs almost nothing; a delete-heavy table that is scanned a lot pays on every scan until the cleanup runs.

> [!NOTE] Buffering is bounded on purpose
> An unbounded set of tombstones would make every check slower and every page fuller. A fixed small buffer bounds both: at most `TOMBS` dead pairs per leaf, at most `TOMBS` comparisons per check, and the oldest deleted key is really removed by the delete that needs the room.

## C++ comparison

| C / C++ | Rust |
|---|---|
| a `deleted` flag per slot (a bitmap) | a small list of deleted keys: no per-pair flag, no extra byte per entry |
| `std::vector<bool>` / `std::bitset` | `Vec<K>` (the buffer, decoded from the page) |
| `ERASE` in a hash table with open addressing leaving a "deleted" marker | the same idea; Robin Hood hashing's backward shift is the alternative that avoids markers |

## In real code

### Using it: the delete rules on a tiny sorted array

The whole policy (buffer of the last N deletes, the oldest removed for real, insert bringing a key back, a scan that skips) on a `Vec`, small enough to read at once and with the exact rules above. The page version stores the same state in bytes.

```rust test
struct Leaf {
    pairs: Vec<(i32, &'static str)>,     // sorted by key; includes deleted pairs
    tombs: Vec<i32>,                     // the buffer, oldest first
    cap: usize,
}

impl Leaf {
    fn new(cap: usize, pairs: &[(i32, &'static str)]) -> Leaf { Leaf { pairs: pairs.to_vec(), tombs: vec![], cap } }

    fn slot(&self, key: i32) -> Option<usize> { self.pairs.binary_search_by_key(&key, |p| p.0).ok() }
    fn is_deleted(&self, key: i32) -> bool { self.tombs.contains(&key) }

    fn get(&self, key: i32) -> Option<&'static str> {
        let at = self.slot(key)?;
        (!self.is_deleted(key)).then(|| self.pairs[at].1)
    }

    fn delete(&mut self, key: i32) -> bool {
        if self.slot(key).is_none() || self.is_deleted(key) { return false; }
        if self.tombs.len() == self.cap {
            let oldest = self.tombs.remove(0);                         // make room: this pair really goes (one shift)
            let at = self.slot(oldest).unwrap();
            self.pairs.remove(at);
        }
        self.tombs.push(key);
        true
    }

    fn insert(&mut self, key: i32, value: &'static str) -> bool {
        match self.slot(key) {
            Some(at) if self.is_deleted(key) => { self.pairs[at].1 = value; self.tombs.retain(|&t| t != key); true }   // back to life
            Some(_) => false,                                                                                       // a live duplicate
            None => { let at = self.pairs.partition_point(|p| p.0 < key); self.pairs.insert(at, (key, value)); true }
        }
    }

    fn scan(&self) -> Vec<i32> { self.pairs.iter().map(|p| p.0).filter(|k| !self.is_deleted(*k)).collect() }
}

#[test]
fn deletes_buffer_and_the_oldest_is_removed_for_real_when_the_buffer_is_full() {
    let mut l = Leaf::new(2, &[(1, "a"), (2, "b"), (3, "c"), (4, "d"), (5, "e"), (6, "f")]);
    assert!(l.delete(3) && l.delete(4));
    assert_eq!((l.pairs.len(), l.tombs.clone()), (6, vec![3, 4]), "nothing moved");
    assert!(!l.delete(3), "already deleted");
    assert!(!l.delete(9), "never there");
    assert!(l.delete(5));                                              // the buffer was full: 3 goes for real
    assert_eq!((l.pairs.len(), l.tombs.clone()), (5, vec![4, 5]));
    assert_eq!(l.slot(3), None);
    assert_eq!(l.scan(), vec![1, 2, 6]);
    assert_eq!(l.get(4), None);
    assert_eq!(l.get(2), Some("b"));
}

#[test]
fn insert_brings_a_deleted_key_back_without_a_new_slot() {
    let mut l = Leaf::new(2, &[(1, "a"), (2, "b"), (3, "c")]);
    l.delete(2);
    assert!(!l.insert(3, "x"), "a live duplicate is refused");
    assert!(l.insert(2, "B"), "a deleted key comes back");
    assert_eq!((l.pairs.len(), l.tombs.len()), (3, 0));
    assert_eq!(l.get(2), Some("B"));
    assert!(l.insert(0, "z"));
    assert_eq!(l.scan(), vec![0, 1, 2, 3]);
}
```

```rust test
use std::collections::BTreeMap;

/// The same idea in a log-structured store: deletes append a tombstone; reads take the newest entry; compaction drops the dead data.
#[derive(Clone, Debug, PartialEq)]
enum Entry { Put(&'static str), Tombstone }

struct Lsm { runs: Vec<BTreeMap<i32, Entry>> }          // newest run last; each run is immutable once written

impl Lsm {
    fn new() -> Lsm { Lsm { runs: vec![BTreeMap::new()] } }
    fn put(&mut self, k: i32, v: &'static str) { self.runs.last_mut().unwrap().insert(k, Entry::Put(v)); }
    fn delete(&mut self, k: i32) { self.runs.last_mut().unwrap().insert(k, Entry::Tombstone); }   // a write, not an erase
    fn flush(&mut self) { self.runs.push(BTreeMap::new()); }
    fn get(&self, k: i32) -> Option<&'static str> {
        for run in self.runs.iter().rev() {                // the newest entry wins; a tombstone hides the older put
            match run.get(&k) { Some(Entry::Put(v)) => return Some(v), Some(Entry::Tombstone) => return None, None => {} }
        }
        None
    }
    /// Merge all runs into one; the tombstones, and what they hid, are finally dropped.
    fn compact(&mut self) {
        let mut merged: BTreeMap<i32, Entry> = BTreeMap::new();
        for run in &self.runs { for (k, e) in run { merged.insert(*k, e.clone()); } }
        merged.retain(|_, e| *e != Entry::Tombstone);
        self.runs = vec![merged, BTreeMap::new()];
    }
}

#[test]
fn a_tombstone_hides_older_data_until_compaction_drops_both() {
    let mut db = Lsm::new();
    db.put(1, "a"); db.put(2, "b");
    db.flush();                                            // the first run is now immutable
    db.delete(1);                                          // cannot touch the old run: write a tombstone in the new one
    assert_eq!(db.get(1), None);
    assert_eq!(db.get(2), Some("b"));
    assert_eq!(db.runs[0].len() + db.runs[1].len(), 3, "the dead data and the tombstone are both still stored");
    db.compact();
    assert_eq!(db.runs[0].len(), 1, "after compaction only the live key is left");
    assert_eq!((db.get(1), db.get(2)), (None, Some("b")));
}
```

### In the exercises

- **2d-01, 2d-02:** the buffer (oldest first, bounded by `TOMBS`) as part of your leaf page, `remove` as a logical delete, `insert` with the "back to life" arm, the scan skipping tombstones; and what moving pairs between pages (split, borrow, merge) must do with their tombstones.
- **2d-03:** a full leaf reclaims its dead pairs, oldest first, before it splits: the compaction every lazy-delete design ends up needing.
- **Boss:** BusTub's four tombstone tests check exactly these rules, in this order.

### Where it is used

- **LSM-trees** (RocksDB, LevelDB, Cassandra, HBase, ScyllaDB): every delete is a tombstone record; compaction removes tombstones and the data they hide (Cassandra keeps them for `gc_grace_seconds` so that replicas that missed the delete cannot bring the data back).
- **PostgreSQL**: a deleted row version stays in the table until `VACUUM`; in B-tree indexes, entries found dead are marked (`LP_DEAD`, the `kill_prior_tuple` optimisation) and removed lazily, for instance when a page is about to split.
- **Hash tables with open addressing** (`hashbrown` uses `DELETED` control bytes where an empty one would break a probe chain); Robin Hood hashing avoids them with a backward shift.
- **Soft deletes** in applications (`deleted_at` columns) and **CRDTs** (an observed-remove set keeps tombstones so concurrent adds and removes merge correctly).
