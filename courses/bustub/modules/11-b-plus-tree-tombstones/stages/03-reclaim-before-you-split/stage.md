Tombstones make a delete cheap, and they make a leaf look fuller than it is. A leaf of 5 slots that holds 4 pairs, 3 of them deleted, has one live key and reports a size of 4; the next insert brings it to 5 and splits it. The tree now has two half-empty leaves where one mostly empty leaf would do, a new separator in the parent, and on a workload that deletes and inserts in the same key range, it keeps doing that: the tree grows deeper while the number of live keys stays flat. The fix is the rule every lazy-delete design ends up with: **when a full leaf holds dead pairs, reclaim their slots before you split it**. This stage adds that rule to `insert`.

> [!CHECK] Once this stage is done, can a leaf that holds tombstones still split? What does your answer mean for the code you wrote in 2d-02 to divide a tombstone buffer between the two halves of a split, and what would you do with that code?
> ||No. A leaf splits only on the insert that would bring it to `leaf_max_size`, and from now on that insert first reclaims the leaf's dead pairs; at least one goes, so the leaf does not reach its maximum and does not split. A leaf with no tombstones splits as before, and its buffer is empty, so dividing the buffer between the halves is a step that always moves nothing. The code is now unreachable: you can keep it as a guard, or replace it by a `debug_assert!` that the buffer of a leaf being split is empty, which turns the reasoning above into a checked invariant. Removing a code path by making an invariant stronger is a common move in storage engines.||
>
> - When does a leaf split?
> - What does the insert that would split it do first now?
> - What is in the buffer of a leaf that splits?

## The task

For a tree with `TOMBS > 0`, an insert of a key that is **not already in its leaf** (neither live nor deleted), into a leaf that would reach `leaf_max_size` with it:

- If the leaf holds tombstones, it **reclaims** dead pairs, the **oldest first**: they are removed for real, with their tombstones, and the new pair goes in. The leaf does **not** split, the tree's shape does not change, and the remaining tombstones are the newest ones, in their order.
- How many to reclaim is yours (see below), but at least one, and never so many that the leaf, once the new pair is in, holds fewer than its minimum of `leaf_max_size / 2` pairs.
- If the leaf holds no tombstones, it splits as in 2c-02.
- An insert that revives a deleted key, or that is refused because the key is live, needs no slot: it reclaims nothing.
- Lookups, scans and every rule of 2d-01 and 2d-02 are unchanged.

## Your freedom

How many dead pairs to reclaim: exactly one (each insert into the full leaf pays for one slot), every one the leaf can lose and stay at its minimum (one pass now, several cheap inserts later), or all of them when that keeps the leaf at its minimum. Whether you reclaim before the insert or after it, and whether the decision lives in the tree or in your leaf page.

## The Rust toolbox

**Oldest first is a prefix.** The buffer is kept oldest first, so "the `n` oldest tombstones" is `tombstones().into_iter().take(n)`, and what remains is the suffix.

**A bound you can name.** `leaf_max_size - min_size` is how many pairs a full leaf can lose and still hold its minimum once the new pair is in. Compute it once, with a comment that says why, instead of scattering `- 1`s.

**`debug_assert!` for a rule you just made true.** `debug_assert!(leaf.tombstones().is_empty(), "a leaf with tombstones reclaims instead of splitting")` at the top of the split costs nothing in release builds and fails loudly in tests if the rule ever stops holding.

**Look before you reclaim.** Whether the key is already in the leaf (live or deleted) is a lookup you have; do it first, so that a revival or a refused duplicate does not cost a dead pair its slot.

## If this is new

- [S3 Vec & slices](/t/s3-vec-slices): `take`, prefixes and suffixes.
- [S1 Option & Result](/t/s1-option-result): telling "already here" from "not here" with `Option`.
- The optional *tombstones and lazy deletion* concept: why every lazy-delete design compacts eventually.

## Tests

- A full leaf with tombstones reclaims the oldest dead pairs instead of splitting; a full leaf without tombstones still splits.
- Bringing a deleted key back into a full leaf, or a refused duplicate, reclaims nothing.
- A stream of deletes and new keys in one leaf never splits it.
- Property: on random shapes and buffer sizes, an insert that adds a leaf only ever split one without tombstones, and every rule of 2d-02 holds after every step.

## Hints

### Which inserts need a slot?

Three kinds of insert reach a full leaf: a new key, a deleted key coming back, and a live key (refused). Only the first takes a slot. Write the three cases down before you decide where the reclaim goes; doing it before the duplicate check is the commonest bug, and the tests have a case for it.

### Why not all of them?

Reclaiming every tombstone is the simplest rule, and it breaks a rule the shape checker enforces. Take `leaf_max_size = 4` (minimum 2) and a leaf of 3 pairs, all deleted: what does it hold after reclaiming all three and adding the new key? That is why the bound is the leaf's minimum.

### The split code from 2d-02

After this stage no test reaches it with a non-empty buffer. If a test still shows a split carrying tombstones, the reclaim did not run for that insert: print the leaf before and after the call and check the "not already in the leaf" test.

## Performance

A reclaim removes pairs from a leaf already latched for the insert: a shift of the pairs after each removed one, at most a page of copying, and no other page touched. A split instead writes three pages and adds a separator to the parent. On a delete-and-insert workload in a narrow key range, reclaiming keeps the tree at its live size; splitting grows it by about a leaf for every `L / 2` inserts.

**Measure it.** Run 100 000 rounds of "delete the oldest key, insert a new one" on a 1 000-key window with `TOMBS = 4`, once with reclaim and once without, and compare the leaf count and the page writes per round. Predict both before you run.

## Experiment

Optional. Write your prediction down first, then run.

1. **One or many.** Reclaim exactly one dead pair per insert, then as many as the bound allows. Which does fewer page writes on the stream test, and which leaves more dead pairs in the tree?
2. **Reclaim on the fast path.** A reclaim changes only the leaf. In module 2e, should the optimistic insert treat a full leaf with tombstones as safe? Predict what the latch counts would show.

## Other designs

- **Reclaim before split (ours).** Dead pairs are compacted exactly when their space is needed.
- **Compaction by threshold.** A leaf whose share of dead pairs passes a threshold is compacted on any write (challenge 2d-c2 is this rule on one leaf).
- **Background compaction.** A sweeper purges leaves off the critical path; inserts never pay, at the cost of a scheduler and of space until it runs.
- **Opportunistic deletion.** PostgreSQL's nbtree removes index entries already known to be dead when a page would otherwise split, the same idea driven by visibility instead of a buffer.

## In BusTub

BusTub's tombstone buffer is bounded and retires the oldest entry when it is full. Its `TombstoneSplitTest` inserts into a full leaf with tombstones until it sees a second leaf or runs out of keys, and checks only that every buffer holds its own leaf's deleted keys in order, so it passes whether the leaf splits or reclaims. This stage makes the reclaim a rule.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::vector::erase` over a prefix of the buffer | `into_iter().take(n)` and a physical remove per key |
| `assert(num_tombstones == 0)` before a split | `debug_assert!` with a message |
| `if constexpr (NumTombs > 0)` around the reclaim | `if TOMBS > 0`, removed by the compiler when it is 0 |

**Port rule:** a compile-time branch on a template parameter becomes an ordinary `if` on a const generic; the optimiser removes the dead arm.

## Learn more

- [PostgreSQL nbtree README](https://github.com/postgres/postgres/blob/master/src/backend/access/nbtree/README): deleting dead entries before a page split
- [`debug_assert!`](https://doc.rust-lang.org/std/macro.debug_assert.html) · Designing Data-Intensive Applications, chapter 3: compaction
