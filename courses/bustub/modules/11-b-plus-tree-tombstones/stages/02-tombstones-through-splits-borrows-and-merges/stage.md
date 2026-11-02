Logical deletes are easy on one leaf. The difficulty is everything else a B+ tree does to leaves: it **splits** them, **borrows** a pair from one into another, and **merges** two into one. Each must carry the tombstones along correctly: a tombstone belongs to a pair in *its* leaf, no buffer may overflow, and a tree full of tombstones must still keep every structural rule of module 2c. This stage is the rules for that, and they are specific.

> [!CHECK] A leaf with a tombstone buffer of 1 holds the keys 0, 1, 2 with 1 tombstoned, and it is a leaf at its minimum size (3). Another delete makes it short. Why can the tree not simply borrow a pair from the neighbour straight away? What should it do first, and what does that do to the pair count?
> ||The leaf's *size* counts the tombstoned pair as well, so it believes it has 3 pairs when only 2 are live; borrowing or merging while keeping a dead pair around moves and stores garbage. The rule: a short leaf first **purges its own tombstones** (really removes the dead pairs), which may make it even shorter, and only then borrows or merges, moving only live pairs. That is the one place a delete shrinks the physical size of a leaf.||
>
> - What does a short leaf do with its own tombstones before rebalancing?
> - What happens to a tombstoned pair that a sibling lends?
> - Which tombstones survive a merge?

## The task

The rules, with `TOMBS = T`:

- **Split.** When a leaf reaches `leaf_max_size` pairs (tombstoned ones count) it splits as in 2c-02. Each tombstone goes **with its pair**: the keys of the lower half's pairs stay in the left buffer, the rest go to the right buffer, **each buffer keeping its order**.
- **Short leaf.** After a delete a leaf (not the root) is short if its size is below the minimum (`leaf_max_size / 2`). It first **purges its own tombstones**: it really removes the tombstoned pairs and empties its buffer. Then it borrows or merges as before, counting only what is physically there.
- **Borrow.** A pair taken from a sibling that was tombstoned there **keeps its tombstone**: it is added to the borrower's buffer. If the borrower's buffer is full, the dead pair is simply dropped instead of moved.
- **Merge.** The surviving page gets the tombstones of both, the destination's first, then the source's. If together they exceed `T`, the **oldest** tombstoned pairs that do not fit are really removed.
- Internal pages know nothing about tombstones; the optimistic fast path (a delete that fits in the buffer does not touch anything else) still applies.

The tests: BusTub's own scenarios as exact leaf renderings (a split divides the buffer by key; a short leaf purges its own tombstones and merges; a merge keeps the right tombstones; a borrower takes a deleted pair's tombstone along), a test that deletes which fit in the buffers never change the shape of the tree, a test that once the buffers overflow dead pairs go for real and leaves merge away, and a property over **random shapes and buffer sizes 1 to 3** that after every operation `check_shape_t` holds: keys sorted, every tombstone is a key of its own leaf, no duplicate tombstones, no overflow, leaf sizes and depth right, every lookup `depth() + 1` latches, and a scan equals the live keys.

## Your freedom

How you carry tombstones across the operations (rebuild the buffers from keys, or move slots), the order in which you purge and rebalance, and how the checker-visible invariants are kept. The exact buffers after BusTub's scenarios are specified above; for everything else only the invariants are tested.

## The Rust toolbox

**Split a buffer by key.** `leaf.tombstones().into_iter().partition(|t| leaf.lower_bound(t, &cmp) < keep)` gives the keys that stay and the keys that go, each in their original order: `partition` keeps order.

**Extend and trim.** For a merge: `let mut tombs = dest.tombstones(); tombs.extend(src.tombstones()); let overflow = tombs.len().saturating_sub(T); let doomed: Vec<K> = tombs.drain(..overflow).collect();` the drained front is the oldest.

**Remove physically by key.** `leaf.remove(&key, &cmp)` that also drops the key from the buffer (a pair and its tombstone leave together) is the one primitive every rule uses.

**Invariants as a function.** Write your own `check()` that asserts the five rules of the checker; call it in debug builds after each page operation. The shape checker in the tests is the same idea from outside.

**Tests for every `T` with one generic function.** `fn run<const T: usize>(..)` called as `run::<1>`, `run::<2>`, `run::<3>`: const generics make a test parametrised over a type-level number easy.

## If this is new

- [L5 Generics & associated types](/t/l5-generics): const generic functions.
- [S3 Vec & slices](/t/s3-vec-slices): `drain`, `partition`, `extend`.
- Module 2c-04's rebalancing is what you extend.
- Y5 Testing & verification: Understand it: an equivalence property over every buffer size.

## Tests

- A split sends each tombstone with its pair.
- A short leaf purges its own tombstones and merges (BusTub's borrow scenario).
- A merge keeps the tombstones of the page that stays (BusTub's coalesce scenario).
- A borrowed deleted pair takes its tombstone along.
- Deletes that fit in the buffers never change the tree's shape; overflowing buffers merge leaves away.
- Property: for random shapes, buffer sizes 1 to 3 and operations, every structural rule holds after every step.

## Hints

### Redraw BusTub's four scenarios

Each is in the stage's tests as a text rendering. Reproduce each by hand (leaves and buffers after every call) before coding; they are the specification, and the shape checker checks the rest.

### Where do the pairs count?

Everywhere a size is compared (split trigger, short leaf, has-a-spare for borrowing), the size is the **physical** one. Decide for each comparison whether it should be the physical or the live count, and check against the rules above.

### A tombstone for a pair that is gone

The invariant "every tombstone is a key of its own leaf" breaks if you remove a pair and forget its tombstone, or move a tombstone without its pair. The checker finds it with the leaf and the key.

## Performance

Tombstones trade structure work for space: until a buffer overflows, deletes cost almost nothing and nothing rebalances. The price is dead pairs the tree carries and scans step over. A workload that deletes in bursts (one leaf at a time) overflows the buffers fastest.

**Measure it.** Delete 90% of 100 000 keys at random and at sequential positions with `TOMBS = 0` and `TOMBS = 4`; count pages written and merges. Predict which pattern benefits from tombstones.

## Experiment

Optional. Predict first, then run.

1. **Forget the purge.** Skip purging on a short leaf and run the property. Which rule breaks first, and what is the shortest operation sequence the shrinking finds?
2. **Drop the tombstone on borrow.** Do not carry a borrowed pair's tombstone. What does a scan show afterwards?

## Other designs

- **Purge, then rebalance (ours, and BusTub's).**
- **Rebalance on the physical size without purging.** Moves dead pairs around; wasteful and confusing.
- **Periodic compaction.** A background sweep purges leaves; deletes never restructure; needs a scheduler.
- **Tombstones in internal pages** too, for deleting whole subtrees: used by some LSM-flavoured B-trees.

## In BusTub

BusTub's spec for tombstones in the tree: purge the underflowing leaf's tombstones before borrowing or merging, carry tombstones along with borrowed pairs, and on a merge drop the oldest pairs that do not fit. The scenarios are `TombstoneBasicTest`, `TombstoneSplitTest`, `TombstoneBorrowTest` and `TombstoneCoalesceTest`.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::partition` / `stable_partition` | `Iterator::partition` (stable) |
| `tombstones.erase(begin, begin + n)` | `drain(..n)` |
| tests instantiated per `NumTombs` with macros | a generic `run::<T>()` |

**Port rule:** instantiating a test for several template arguments becomes calling a const-generic function with several arguments.

## Learn more

- [`Iterator::partition`](https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.partition) · [`Vec::drain`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.drain)
