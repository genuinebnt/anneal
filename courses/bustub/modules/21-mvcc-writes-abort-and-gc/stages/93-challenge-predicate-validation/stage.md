A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`validate` in `src/concurrency/predicate_validation.rs`: a serializable transaction remembers the **predicates** it read with (`a = 5`, `a BETWEEN 10 AND 20`). At commit, it checks every write made by transactions that committed since its snapshot: if the old row or the new row of a write satisfies one of its predicates, the write could have changed what the predicate returned, and the transaction must abort.

## Why

Locking rows misses *phantoms*: a row inserted by someone else that your range query would have returned. Validating predicates catches them, and the subtle part is the two images: a write that *moves* a row out of your range (old image matches) matters as much as one that moves it in (new image matches).

## The contract

- `Pred::Eq(col, v)` and `Pred::Range(col, lo, hi)` (inclusive); a row is a slice of `i64`.
- `Write { old: Option<Vec<i64>>, new: Option<Vec<i64>> }` (insert has no `old`, delete has no `new`).
- `validate(preds, writes)` is `true` (no conflict) iff **no** write has an image (old or new) that satisfies any predicate.

## Invariants

These must hold after every step, whatever the input:

- Order of predicates and writes does not matter.
- A write with no images (both `None`) conflicts with nothing.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Adding a predicate or a write can only turn `true` into `false`.
- A write that does not touch the predicate columns' values (both images equal and outside) never conflicts.
- An insert into the range conflicts; a delete from the range conflicts; an update from outside to outside does not.

## Examples

Worked cases (the tests include them):

```text
pred a in [10,20]: insert row [15] -> conflict; update [5] -> [8] -> fine; update [15] -> [30] -> conflict
```

## What the tests check

- Equality and range predicates; each image.
- No predicates and no writes.
- A property against a brute-force check.

## Done when

All the `s4b_c4` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Transactions on the key space](/cookbook/p/transactions-on-the-key-space): serializable by validating every point read and scanned range at commit.
- [Timestamp ordering and OCC](/cookbook/p/timestamp-ordering-and-occ): phantoms, and predicate and index locking as the alternatives.
