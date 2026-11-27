A challenge: no walkthrough, no hints, no solution. It is extra practice and does not count towards the course.

## What to fix

`src/execution/aggregates.rs` has `count_star`, `count_col`, `sum` and `avg` over a column that may contain NULLs. Three are right; `avg` is wrong on columns with NULLs. Find the bug and fix it.

## Why

An aggregate that is wrong only when a column holds NULLs is wrong exactly when a report is wrong without anyone noticing.

> [!CHECK] Once the tests pass: what was the bug, in one sentence, and why did ordinary inputs not show it?
> ||`avg` divided the sum by `COUNT(*)`, which counts NULL rows, instead of by `COUNT(col)`. Every aggregate except `COUNT(*)` ignores NULLs, and the average of nothing but NULLs is NULL.||

## The contract

- `count_star` counts all rows. `count_col` counts non-NULL values.
- `sum` adds the non-NULL values; `None` when there are none.
- `avg` is the sum of the non-NULL values divided by their count; `None` when there are none.

## Invariants

These must hold after every step, whatever the input:

- `avg * count_col == sum` (up to floating-point error) when `count_col > 0`.
- `count_col <= count_star`.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Adding NULLs to the column changes `count_star` and nothing else.
- Removing the NULLs first gives the same `sum`, `count_col` and `avg`.
- A column of only NULLs: `sum` and `avg` are `None`, `count_col` is 0.

## Examples

Worked cases (the tests include them):

```text
[1, NULL, 3] -> count_star 3, count_col 2, sum 4, avg 2.0
[NULL, NULL] -> 2, 0, None, None
```

## What the tests check

- Columns with and without NULLs.
- All NULLs and empty.
- A property: NULLs are invisible to everything but `count_star`.

## Done when

All the `s3f_c4` tests pass, and you can say in one sentence what the bug was.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Modern SQL](/cookbook/p/modern-sql): COUNT(*) versus COUNT(x), and aggregates over no non-NULL values being NULL.
