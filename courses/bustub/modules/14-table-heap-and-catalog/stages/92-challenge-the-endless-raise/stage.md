A challenge: no walkthrough, no hints, no solution. It is extra practice and does not count towards the course.

## What to fix

`give_raise` in `src/storage/table/raise.rs` implements `UPDATE emp SET salary = salary * factor WHERE salary < limit` on a heap that only appends: it marks the old row version dead and appends the new version at the end. It looks right, and some employees get the raise twice (or the loop does not end). Find the bug and fix it.

## Why

This bug has a name and a date: IBM engineers first saw it in 1976, and every database that updates rows by moving them has to defend against it.

> [!CHECK] Once the tests pass: what was the bug, in one sentence, and why did ordinary inputs not show it?
> ||The Halloween problem: the scan ran to the *current* end of the heap, so a raised row appended at the end was found again and raised again. Scan only the rows that existed when the scan started, or collect the updates first and apply them afterwards.||

## The contract

- `give_raise(rows, limit, factor)`: every live row whose salary is below `limit` gets its salary multiplied by `factor` **exactly once**; the old version is marked dead (its slot becomes `None`) and the new one appended.
- Returns the number of rows updated.

## Invariants

These must hold after every step, whatever the input:

- Every employee appears once among the live rows afterwards.
- Rows that did not match are untouched.
- The scan ends.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- The set of live salaries equals applying the raise once to every row below the limit.
- The result does not depend on the order the rows are scanned.
- Running with `factor = 1` leaves the live salaries unchanged.

## Examples

Worked cases (the tests include them):

```text
[(1, 100), (2, 5000)], limit 1000, factor 2 -> live: (2, 5000), (1, 200); updated 1
```

## What the tests check

- One matching row; none; many.
- A factor that keeps salaries below the limit (the loop would not end).
- A property against a map.

## Done when

All the `s3c_c3` tests pass, and you can say in one sentence what the bug was.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Table heap](/cookbook/p/table-heap): the Halloween problem in a table heap, and a scan that avoids it.
- [Access-method executors](/cookbook/p/access-method-executors): Halloween safety in the Update and Delete executors.
