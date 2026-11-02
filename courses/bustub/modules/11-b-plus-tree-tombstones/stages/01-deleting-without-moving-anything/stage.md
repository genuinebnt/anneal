Removing a pair from the middle of a leaf shifts every later pair one slot left: for a 511-pair leaf that is up to 8 KiB of copying per delete. A **tombstone** is a cheaper delete: leave the pair where it is and write its key down in a small **buffer** in the leaf. Lookups and scans treat a tombstoned key as absent; an insert of the same key later brings the pair back to life with the new value. The buffer is bounded (`TOMBS` keys, a property of the tree's *type*, BusTub's `NumTombs`): when it is full, the **oldest** tombstoned pair is removed for real to make room. This module changes what `remove` does, how `insert` treats a deleted key, and what scans and lookups see, without changing what the tree answers.

> [!CHECK] A leaf has a tombstone buffer of 2 and holds the keys 10, 20, 30, 40. You delete 20, then 30, then 40. Which keys are in the leaf after each delete, which are tombstones, and what do a lookup and a scan show at the end? Which delete was the first to move any pair?
> ||After 20: pairs 10, 20, 30, 40; tombstones [20]. After 30: same pairs; tombstones [20, 30]. The buffer is full when 40 is deleted, so the oldest tombstone (20) is retired: that pair is really removed (the first and only physical move), and the tombstones become [30, 40] over the pairs 10, 30, 40. A lookup of 20, 30 or 40 finds nothing and a scan shows only 10. Nothing moved until the buffer overflowed.||
>
> - What is stored where when a key is tombstoned?
> - What decides which pair is removed for real?
> - What does inserting 30 again do now?

## The task

For a tree with `TOMBS > 0` (the tests use 1 to 4), in your leaf pages and in `remove`, `insert`, `get_value`, `begin`, `begin_at`:

- **Delete.** `remove(&key)` for a live key in a leaf: if the leaf's buffer is full, first **really remove** the pair of the *oldest* tombstone (and drop that tombstone); then add `key` as the newest tombstone. The pair stays in the leaf. A key that is absent or already tombstoned: nothing happens.
- **Lookup and scan.** A tombstoned key is **not found** by `get_value` and **not returned** by a scan; `begin_at(k)` starts at the first *live* key not less than `k`.
- **Insert.** Inserting a key that is tombstoned **revives** it: the pair keeps its slot, takes the new value, and its tombstone goes. It succeeds (true). Inserting a live key is a duplicate (false).
- **Counts.** A leaf's size counts every pair physically stored, tombstoned or not. With `TOMBS = 0` deletes are physical and nothing changes.
- **Observers.** `leaf_keys()` returns the keys physically stored in each leaf (tombstoned ones included), `leaf_tombstones()` each leaf's buffer, oldest first. The leaf page type gains the buffer: the room it takes comes out of the room for pairs (`default_leaf_max_size` shrinks by the buffer size).
- A delete that fits in the buffer, and the insert that revives, still write-latch **only the leaf** (the optimistic path of 2c-05).

Tests, written with a text rendering of the leaves (`[0,1][2,3~2,3][4,5~4]`: three leaves, tombstones after the `~`): exact scenarios for each rule above, a tree whose every pair is deleted (empty to a scan, but its page still there), the one-write-latch count, a tree without tombstones reports no tombstones, and a property: on one big leaf, any inserts and removes agree with a set, every tombstone belongs to a pair in its leaf, and a buffer never overflows.

## Your freedom

Where the buffer lives in the page (before the pairs is the obvious place; after them works too), how many bytes you reserve and in what form, how you find a tombstoned key (a scan of the buffer or a flag per pair), and how `remove` is organised. Whether tombstones are keys or slot numbers is yours; the observers return keys.

## The Rust toolbox

**A const generic decides the layout.** `const TOMBS: usize` on the leaf type: `const TOMB_REGION: usize = if TOMBS == 0 { 0 } else { 4 + TOMBS * K::SIZE };` is an associated constant computed from the parameters, so the pairs start at `HEADER + TOMB_REGION` and a leaf of one `TOMBS` cannot be misread as another.

**`if TOMBS > 0` costs nothing when it is false.** The compiler removes the branch for `TOMBS = 0`: write the tombstone path inline and let the plain tree pay nothing.

**Keys, not slot numbers.** A key in the buffer survives pairs shifting; a slot number would have to be adjusted every time a pair moves. The cost is a binary search to find the pair when you need it.

**`Vec<K>` for the buffer in code.** Read the buffer into a `Vec<K>` (`tombstones()`), change it, write it back (`set_tombstones(&[K])`): it is at most `TOMBS` keys, and the code is easy to read.

**Compare through the comparator.** Two keys are the same key if `cmp.compare(a, b) == Equal`, not if their bytes match (a `GenericKey` may carry junk after the integer).

## If this is new

- [L5 Generics & associated types](/t/l5-generics): const generics and associated constants.
- S8 The core traits: `Ord`/comparators and equality.
- The optional *tombstones and lazy deletion* concept explains the idea.
- L7 Enums & pattern matching: Enums as design: a state (live or deleted) that is part of an entry.

## Tests

- Remove buffers a tombstone instead of shifting pairs; removing a deleted or missing key changes nothing.
- Deleted pairs are not found and not scanned; a scan from a deleted key starts at the next live one.
- A full buffer makes room by really removing the oldest.
- Inserting a deleted key again revives it with the new value.
- A tree with every pair deleted is empty to a scan but keeps its page.
- A fitting delete, and a revive, write-latch one page.
- A tree without tombstones reports none.
- Property: logical deletes agree with a set on one leaf for buffers of 1 to 3.

## Hints

### Work through the check-yourself example on paper

Draw the leaf with its buffer after each delete. Which line of your `remove` does each step correspond to?

### Do the scans and lookups know?

Every place that reads a pair must ask "is it tombstoned?": `get_value`, the iterator's `next`, `begin_at`'s lower bound, and `insert`'s duplicate check. List them before you start; forgetting one is the commonest bug.

### Capacity

If a leaf holds 511 pairs without tombstones and your buffer takes `4 + TOMBS * 8` bytes, how many does it hold now? `default_leaf_max_size` must say so, or your leaf will overflow its page.

## Performance

A logical delete is a binary search and an append to the buffer (a few bytes written), against a shift of up to 8 KiB. Reads cost one extra check per key. The price is space (the buffer, and dead pairs that stay until the buffer overflows) and a scan that must step over dead pairs.

**Measure it.** Delete a thousand keys from the middle of a 500-pair leaf with `TOMBS = 0` and with `TOMBS = 3`: count bytes moved (or time it) and predict the ratio.

## Experiment

Optional. Predict first, then run.

1. **A big buffer.** Use `TOMBS = 16` and delete 200 keys in a 500-pair leaf. How many pairs are still stored, and what does a lookup of a live key cost now?
2. **Forget the scan.** Make the iterator ignore tombstones. Which tests fail and with what message?

## Other designs

- **Tombstone keys in a buffer (ours).** Bounded, ordered by age, simple.
- **A deleted flag per pair.** Unbounded dead pairs; needs a bit per pair and a sweep.
- **Version chains** (MVCC, module 4): a delete is a new version; garbage collection frees the old ones.
- **Immediate physical delete.** What module 2c did.

## In BusTub

BusTub's tombstone extension (2025): "Leaf pages also contain a fixed buffer of tombstone indexes for entries that have been deleted" and the layout `| HEADER | TOMB_SIZE | TOMB(0) ... | entries |`. This course stores keys instead of indexes, which survive shifts.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `template <..., size_t NumTombs = 0>` | `const TOMBS: usize = 0` |
| `#define LEAF_PAGE_TOMB_CNT` | an associated `const` |
| `size_t tombstones_[LEAF_PAGE_TOMB_CNT]` (zero-length when 0) | a region of 0 bytes when `TOMBS` is 0 |

**Port rule:** a template parameter that changes a struct's size becomes a const generic; a macro that computes sizes from it becomes an associated const.

## Learn more

- [Const generics](https://doc.rust-lang.org/reference/items/generics.html#const-generics) · [Associated constants](https://doc.rust-lang.org/reference/items/associated-items.html#associated-constants)
- Tombstones in LSM trees and B-trees (a survey of lazy deletion)
