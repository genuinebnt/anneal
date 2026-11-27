---
title: Range scans and the leaf chain
summary: Why the leaves of a B+ tree are linked, what a cursor must remember, how to write one that implements Iterator without holding a latch, and the std tools for the same job in memory.
minutes: 9
---
"All orders between March and June." "The next 50 users after this id." "Everything, sorted." These are *range* queries, and they are what a B+ tree has over a hash table. The trick is in the leaves: they are sorted, and each one knows the **next** leaf. Finding the start of a range costs one root-to-leaf descent; every further pair costs a step along the bottom, with no climbing back up.

```svg
caption: A range scan from key 4. One descent finds the leaf holding 4 (solid arrows). The cursor then walks right along the leaf chain (dashed), reading pairs until the range ends. The upper levels are never touched again.
<svg viewBox="0 0 760 250" role="img" aria-label="A descent from the root to the leaf containing 4, then a walk along the leaf chain to 9">
<defs><marker id="rs-a" markerWidth="9" markerHeight="9" refX="7" refY="4.5" orient="auto"><path d="M0 0 L9 4.5 L0 9 z" style="fill:var(--dim)"/></marker></defs>
<rect class="blue" x="300" y="14" width="160" height="36" rx="4"/><text class="mid t-b sm" x="380" y="37">root: keys 5, 8</text>
<path class="ln-w" d="M340 50 L150 118" marker-end="url(#rs-a)"/><path class="ln dash" d="M380 50 L380 118"/><path class="ln dash" d="M420 50 L620 118"/>
<g>
<rect class="live" x="70" y="120" width="160" height="44" rx="4"/><text class="mid fg" x="150" y="147">3  4</text>
<rect class="live" x="300" y="120" width="160" height="44" rx="4"/><text class="mid fg" x="380" y="147">5  6  7</text>
<rect class="live" x="530" y="120" width="160" height="44" rx="4"/><text class="mid fg" x="610" y="147">8  9  10</text>
</g>
<path class="ln-g dash" d="M230 142 H298" marker-end="url(#rs-a)"/><path class="ln-g dash" d="M460 142 H528" marker-end="url(#rs-a)"/>
<circle class="hot" cx="170" cy="190" r="8"/><text class="mid t-w sm" x="170" y="214">cursor starts at 4</text>
<path class="ln-w" d="M184 190 H600" marker-end="url(#rs-a)"/><text class="mid t-w sm" x="460" y="214">reads 4 5 6 7 8 9 … then stops at the end of the range</text>
</svg>
```

## What a cursor remembers

Two numbers: *which leaf* and *which slot in it*. To move forward: slot + 1; if that is past the leaf's last pair, follow `next_page_id` to slot 0 of the next leaf; if there is no next leaf, the cursor is at the **end**. Keep that position **normalised** (always pointing at a pair that exists, or the end) and everything else is trivial:

- `is_end()` is "no leaf";
- two cursors are equal when leaf and slot match, so `it == tree.end()` works;
- `begin_at(key)` can ask the leaf for `lower_bound(key)`, which may be one past its last pair; normalising moves on to the next leaf.

## To latch or not between steps

A cursor that keeps a read latch on its current leaf blocks every writer of that leaf for as long as the caller dawdles, and in a tree where writers latch left-to-right it can deadlock with a merge. The alternative this course uses: **hold no latch between calls**. Each `next()` latches the leaf, copies the pair out, and lets go. The price: if a writer changes the tree between two calls the scan may skip or repeat keys around the change; for most uses (a single transaction's snapshot, or a table nobody writes during the scan) that is fine, and databases that need more use snapshots or version numbers rather than long latches.

> [!NOTE] Why `next()` returns the pair by value
> A reference into a page is valid only while the page stays latched and pinned. If the iterator held a guard it could return references (like C++'s `operator*`), but the guard would live as long as the iterator. Returning an owned pair is the Rust way to say "I have copied it; nothing is borrowed".

## C++ comparison

| C / C++ | Rust |
|---|---|
| `iterator` class with `operator*`, `operator++`, `operator!=` | `impl Iterator` (`next`) plus `PartialEq`; `for x in it` |
| `std::map::lower_bound(k)` | `BTreeMap::range(k..)`, or a tree's `begin_at(&k)` |
| `[lo, hi)` as two iterators | `begin_at(&lo)` then `take_while(\|(k, _)\| k < hi)` |
| a reverse iterator needs a prev pointer or a stack | `DoubleEndedIterator`; this tree only chains forward |

## In real code

### Using it: a cursor over a chain of leaves

The same logic as the stage, over an arena of leaves in a `Vec`: a position, `skip_past_the_end`, `Iterator`, equality and `begin_at`. Nothing is held between calls because the cursor borrows the leaves only inside `next`.

```rust test
struct Leaf { keys: Vec<i32>, next: Option<usize> }

/// A position in a chain of leaves. `leaf == None` is the end.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Cursor { leaf: Option<usize>, index: usize }

struct Chain { leaves: Vec<Leaf> }

impl Chain {
    /// Moves a position past the end of its leaf on to the first pair that exists (or to the end).
    fn normalise(&self, mut c: Cursor) -> Cursor {
        while let Some(l) = c.leaf {
            if c.index < self.leaves[l].keys.len() { return c; }
            c = Cursor { leaf: self.leaves[l].next, index: 0 };
        }
        Cursor { leaf: None, index: 0 }
    }
    fn begin(&self) -> Cursor { self.normalise(Cursor { leaf: Some(0), index: 0 }) }
    fn end(&self) -> Cursor { Cursor { leaf: None, index: 0 } }
    /// The first key that is not less than `key`: descend to the leaf (here: scan the chain), lower_bound in it, normalise.
    fn begin_at(&self, key: i32) -> Cursor {
        let leaf = (0..self.leaves.len()).find(|&i| self.leaves[i].keys.last().is_some_and(|&last| last >= key)).unwrap_or(self.leaves.len() - 1);
        let index = self.leaves[leaf].keys.partition_point(|&k| k < key);
        self.normalise(Cursor { leaf: Some(leaf), index })
    }
    fn iter(&self, from: Cursor) -> Scan<'_> { Scan { chain: self, at: from } }
}

struct Scan<'a> { chain: &'a Chain, at: Cursor }

impl Iterator for Scan<'_> {
    type Item = i32;
    fn next(&mut self) -> Option<i32> {
        let leaf = self.at.leaf?;
        let key = self.chain.leaves[leaf].keys[self.at.index];       // copy the pair out
        self.at = self.chain.normalise(Cursor { leaf: self.at.leaf, index: self.at.index + 1 });
        Some(key)
    }
}

fn chain() -> Chain {
    Chain { leaves: vec![
        Leaf { keys: vec![1, 2], next: Some(1) },
        Leaf { keys: vec![3, 4, 5], next: Some(2) },
        Leaf { keys: vec![7], next: Some(3) },
        Leaf { keys: vec![8, 9], next: None },
    ] }
}

#[test]
fn a_full_scan_crosses_every_leaf_boundary() {
    let c = chain();
    assert_eq!(c.iter(c.begin()).collect::<Vec<_>>(), vec![1, 2, 3, 4, 5, 7, 8, 9]);
    assert_eq!(c.iter(c.end()).next(), None);
}

#[test]
fn begin_at_finds_the_first_key_not_less_than_the_target() {
    let c = chain();
    let from = |k| c.iter(c.begin_at(k)).collect::<Vec<_>>();
    assert_eq!(from(4), vec![4, 5, 7, 8, 9]);
    assert_eq!(from(6), vec![7, 8, 9], "6 is missing: start at 7, which is in the next leaf");
    assert_eq!(from(0), vec![1, 2, 3, 4, 5, 7, 8, 9]);
    assert_eq!(from(9), vec![9]);
    assert_eq!(from(10), vec![], "past the last key is the end");
    assert_eq!(c.begin_at(10), c.end(), "and equal to the end cursor");
}

#[test]
fn a_range_is_a_start_plus_take_while() {
    let c = chain();
    let range: Vec<i32> = c.iter(c.begin_at(3)).take_while(|&k| k < 8).collect();     // [3, 8)
    assert_eq!(range, vec![3, 4, 5, 7]);
    assert_eq!(c.iter(c.begin()).filter(|k| k % 2 == 1).count(), 5, "iterator adapters work on a cursor for free");
}
```

```rust test
use std::collections::BTreeMap;
use std::ops::Bound::{Excluded, Included, Unbounded};

#[test]
fn the_std_btreemap_has_the_same_range_api() {
    let m: BTreeMap<i32, &str> = [(1, "a"), (3, "c"), (5, "e"), (7, "g"), (9, "i")].into_iter().collect();
    assert_eq!(m.range(3..7).map(|(k, _)| *k).collect::<Vec<_>>(), vec![3, 5]);            // [3, 7)
    assert_eq!(m.range(3..=7).map(|(k, _)| *k).collect::<Vec<_>>(), vec![3, 5, 7]);        // [3, 7]
    assert_eq!(m.range(6..).map(|(k, _)| *k).collect::<Vec<_>>(), vec![7, 9], "6 is missing: starts at the next key");
    assert_eq!(m.range(..=3).map(|(k, _)| *k).collect::<Vec<_>>(), vec![1, 3]);
    assert_eq!(m.range((Excluded(3), Included(7))).map(|(k, _)| *k).collect::<Vec<_>>(), vec![5, 7]);
    assert_eq!(m.range((Unbounded, Excluded(5))).count(), 2);
    assert_eq!(m.first_key_value(), Some((&1, &"a")));
    assert_eq!(m.last_key_value(), Some((&9, &"i")));
    // BTreeMap's iterators are double-ended: a descending scan is `.rev()`
    assert_eq!(m.range(3..).rev().map(|(k, _)| *k).collect::<Vec<_>>(), vec![9, 7, 5, 3]);
}

#[test]
fn an_ordered_index_answers_between_top_n_and_next_after() {
    let ids: BTreeMap<u32, &str> = (100..200).step_by(10).map(|i| (i, "row")).collect();
    // WHERE id BETWEEN 120 AND 150
    assert_eq!(ids.range(120..=150).count(), 4);
    // ORDER BY id LIMIT 3
    assert_eq!(ids.keys().take(3).copied().collect::<Vec<_>>(), vec![100, 110, 120]);
    // the 2 rows after id 135 (keyset pagination: "WHERE id > 135 ORDER BY id LIMIT 2")
    assert_eq!(ids.range((Excluded(135), Unbounded)).take(2).map(|(k, _)| *k).collect::<Vec<_>>(), vec![140, 150]);
    // MAX(id) and the largest id below 125
    assert_eq!(ids.keys().next_back(), Some(&190));
    assert_eq!(ids.range(..125).next_back().map(|(k, _)| *k), Some(120));
}
```

### In the exercises

- **2c-04:** moving past the end of a leaf to the next, `Iterator::next`, `begin_at` as a descent plus `lower_bound`, and why no latch is kept between calls.
- **2e-03:** what a scan that keeps no latch may see while writers split, merge and borrow under it, and how checking each pair against the last key returned keeps it in order.
- **The stage's tests** assert the same facts as the first test above (every leaf boundary), plus "nothing stays pinned between calls".

### Where it is used

- **SQL**: `WHERE x BETWEEN a AND b`, `ORDER BY` served from an index, `MIN`/`MAX`, keyset pagination (`WHERE id > ? ORDER BY id LIMIT n`) and merge joins all start with a descent and then walk the leaf chain.
- **LSM-tree stores** (RocksDB, LevelDB) expose the same `seek(key)` + `next()` cursor over sorted runs; range scans in `redb`, `sled` and `fjall` are the same API.
- **In-memory ordered maps**: `BTreeMap::range`, Java's `TreeMap.subMap`, C++'s `std::map::lower_bound`.
- **Rust**: any type that implements `Iterator` gets `take_while`, `skip`, `filter`, `zip`, `collect` and the rest; a cursor is the right shape for an index scan.
