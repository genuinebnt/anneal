A challenge: no walkthrough, no hints, no solution. It is extra practice and does not count towards the course.

## What to fix

`not_in` in `src/execution/not_in.rs` evaluates `x NOT IN (list)` for a value and a list that may contain NULLs. It looks right, and for some lists it returns TRUE where SQL says the answer is unknown. Find the bug and fix it.

## Why

A `NOT IN` that is wrong only when the list happens to contain a NULL is the kind of query that works for years, and it is wrong in the direction that looks right.

> [!CHECK] Once the tests pass: what was the bug, in one sentence, and why did ordinary inputs not show it?
> ||It dropped the NULLs from the list first, on the reasoning that a NULL can never match. But a NULL in the list means `x` *might* equal it, so when `x` matches no known value the answer is unknown, not TRUE, and SQL returns no rows.||

## The contract

- `not_in(x, list)` is TRUE if `x` is not NULL, differs from every item and the list has no NULL (or is empty); FALSE if `x` equals some item; NULL otherwise.
- `x` NULL with an empty list is TRUE: there is nothing it could equal.

## Invariants

These must hold after every step, whatever the input:

- `not_in` is never TRUE for a non-empty list containing a NULL.
- `not_in(x, list)` is the three-valued negation of `x IN list`.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Removing a non-NULL item from a list can only move the answer towards TRUE.
- Adding a NULL to any list never makes the answer TRUE.

## Examples

Worked cases (the tests include them):

```text
x = 3, list [1, 2]: TRUE
x = 3, list [1, NULL]: unknown (NULL)
x = 1, list [1, NULL]: FALSE
```

## What the tests check

- The textbook cases.
- A NULL in the list.
- A NULL on the left.
- The empty list.
- A property against the definition.

## Done when

All the `s3i_c2` tests pass, and you can say in one sentence what the bug was.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Modern SQL](/cookbook/p/modern-sql): why NOT IN with a NULL in the list is never TRUE.
- [Unnesting subqueries](/cookbook/p/unnesting-subqueries): NOT IN needing a null-aware anti-join, unlike NOT EXISTS.
