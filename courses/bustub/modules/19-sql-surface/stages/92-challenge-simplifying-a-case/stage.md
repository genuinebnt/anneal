A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`fold_case` in `src/optimizer/case_fold.rs`: given a CASE whose conditions are either known (TRUE, FALSE, NULL) or unknown until a row arrives, return a simpler CASE that means exactly the same, or the result outright when it is already decided. Results are opaque ids so the tests can tell them apart.

## Why

ORM-generated queries and rewritten views are full of `CASE WHEN 1 = 1 THEN ...` and `CASE WHEN false THEN ...`. An optimizer that folds them makes the plan smaller, lets predicates inside become index lookups, and avoids evaluating dead branches. The skill is the one every optimizer rule needs: change the shape, prove the meaning is the same, and remember that the *order* of branches is part of the meaning.

## The contract

- A branch whose condition is FALSE or NULL can never be taken and is removed.
- The first branch whose condition is TRUE always wins when reached: it becomes the ELSE, and every branch after it is removed.
- If no branch is left, the CASE is its ELSE (or NULL when there is none): `Folded::Result`. If the first remaining branch is TRUE, the result is its result.
- Otherwise the result is `Folded::Case` with the remaining branches, in their original order.

## Invariants

These must hold after every step, whatever the input:

- Folding preserves meaning for every assignment of the unknown conditions.
- Folding is idempotent.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Folding a folded CASE changes nothing.
- The number of branches never increases.
- A CASE of only unknown conditions is returned unchanged.

## Examples

Worked cases (the tests include them):

```text
[(False, 1), (Unknown(0), 2)] else 3 -> CASE [(Unknown(0), 2)] else 3
[(Unknown(0), 1), (True, 2), (Unknown(1), 3)] else 4 -> CASE [(Unknown(0), 1)] else 2
[(False, 1)] -> NULL
```

## What the tests check

- Dead branches removed.
- A TRUE branch decides everything after it.
- Fully decided CASEs.
- Idempotence and meaning by property.

## Done when

All the `s3i_c3` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Query planning and rewriting](/cookbook/p/query-planning-and-rewriting): folding that is only valid where NULL and FALSE both drop the row.
