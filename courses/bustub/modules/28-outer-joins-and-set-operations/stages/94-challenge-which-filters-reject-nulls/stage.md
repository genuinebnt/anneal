A challenge: no walkthrough, no hints, no solution. It adds to something you built in this module, using what you learned there. It is extra practice and does not count towards the course.

## What to build

`rejects_nulls` in `src/execution/null_rejection.rs`, on a small expression language (columns, integer literals, `COALESCE`, `>`, `=`, `IS NULL`, `IS NOT NULL`, `AND`, `OR`, `NOT`): given the columns that are NULL on a padded row, say whether the condition is **certainly not TRUE** for it, whatever the other columns hold. Stage 3j-03 answered "no" for `NOT`, `COALESCE` and constants; this one answers as precisely as the language allows.

## Why

A "yes" turns an outer join into an inner join, so it must never be wrong: that is *soundness*, and a brute-force check over small values proves it. A "no" only costs an optimization, and the more conditions that get a correct "yes", the more joins are simplified: that is *precision*. The way to get both is to evaluate the condition not on values but on **sets of possible outcomes**: the set of truth values a sub-condition can take, given that some columns are NULL and the others are unknown.

## The contract

- `eval` (given) evaluates a condition on a row of `Option<i64>`, with three-valued logic: the result is `Some(true)`, `Some(false)` or `None` for unknown.
- `rejects_nulls(cond, nulls)`: `nulls` lists the columns that are NULL; every other column may hold any integer or NULL.
- Return `true` only if `eval` is never `Some(true)` for any such row (soundness).
- Return `true` for every condition where this holds and no column appears twice in it (precision on the common case); a condition that repeats a column may get a cautious `false`.

## Invariants

These must hold after every step, whatever the input:

- Soundness: if `rejects_nulls` is `true`, no assignment of the other columns makes the condition true.
- The answer depends only on the condition and on which columns are NULL.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- `NOT` flips true and false and keeps unknown: `NOT (c0 > 3)` rejects when c0 is NULL, `NOT (c0 IS NULL)` rejects when c0 is NULL, `NOT (c0 IS NOT NULL)` does not.
- `AND` rejects if either side does; `OR` only if both do; `COALESCE(c0, 5) > 3` does not reject when c0 is NULL (5 > 3), but `COALESCE(c0, 1) > 3` does.
- Making more columns NULL never turns a `true` into `false` for a condition without `IS NULL`.

## Examples

Worked cases (the tests include them):

```text
c0 > 3, nulls [0] -> true
NOT (c0 > 3), nulls [0] -> true
c0 IS NULL, nulls [0] -> false
COALESCE(c0, 5) > 3, nulls [0] -> false; COALESCE(c0, 1) > 3 -> true
(c0 > 3) OR (c1 > 3), nulls [0] -> false; nulls [0, 1] -> true
```

## What the tests check

- A list of conditions with the expected answers.
- Soundness against a brute-force evaluation, on random conditions.
- Precision on random conditions with no repeated column.

## Done when

All the `s3j_c5` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Plan transformations and rule soundness](/cookbook/p/plan-transformations-and-rule-soundness): null-rejection deciding when an outer join may become inner.
- [Abstract interpretation](/cookbook/p/abstract-interpretation-intervals): sound abstract interpretation over sets of possible values.
