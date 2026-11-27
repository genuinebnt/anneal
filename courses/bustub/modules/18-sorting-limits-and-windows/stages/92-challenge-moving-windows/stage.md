A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`moving_sum` in `src/execution/moving_sum.rs`: `SUM(x) OVER (ORDER BY ... ROWS BETWEEN p PRECEDING AND f FOLLOWING)` for a column of nullable integers already in window order. For each row, the sum of the non-NULL values in the frame of rows `i - p ..= i + f` (clipped to the input), or NULL when the frame has no non-NULL value. Linear time.

## Why

A running total, a 7-day moving average and a rolling maximum are all window frames, and the naive implementation recomputes each frame from scratch: `O(n * frame)`. Prefix sums make a sum frame `O(1)` per row. The details (clipping at the ends, NULLs) are the same ones a real window executor must get right.

## The contract

- The frame of row `i` is rows `max(0, i - p)` to `min(n - 1, i + f)` inclusive.
- NULL values contribute nothing; a frame with no non-NULL value gives NULL.
- Sums wrap on overflow.

## Invariants

These must hold after every step, whatever the input:

- The output has one entry per input row.
- Entry `i` depends only on rows inside its frame.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- `p = 0, f = 0` returns each value itself (NULL stays NULL).
- Widening the frame never removes a non-NULL result.
- `p = n, f = 0` is the running total.

## Examples

Worked cases (the tests include them):

```text
[1,2,3,4], p=1, f=0 -> [1,3,5,7]
[1,NULL,3], p=1,f=1 -> [1,4,3]
[NULL, NULL] -> [NULL, NULL]
```

## What the tests check

- Running totals and centred windows.
- NULLs and the ends.
- A property against the naive frame sum.

## Done when

All the `s3g_c3` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Modern SQL over in-memory tables](/cookbook/p/sql-window-functions-ctes): ROWS frames evaluated with prefix sums of non-NULL values in O(1) per row.
