A challenge: no walkthrough, no hints, no solution. It is extra practice and does not count towards the course.

## What to fix

`src/execution/ranks.rs` computes `ROW_NUMBER`, `RANK` and `DENSE_RANK` for a list of keys already sorted. `row_number` and `dense_rank` are right; `rank` is wrong whenever there are ties. Find the bug and fix it.

## Why

The three ranking functions differ only in what ties do, so a mistake between them shows only when the data has ties.

> [!CHECK] Once the tests pass: what was the bug, in one sentence, and why did ordinary inputs not show it?
> ||`rank` added one at each new key, which is `DENSE_RANK` (1,1,2,3). `RANK` skips after ties (1,1,3,4): a new key's rank is its position, `i + 1`.||

## The contract

- Input keys are non-decreasing. `row_number[i] = i + 1`.
- `rank[i]` is `1 +` the number of rows with a strictly smaller key. `dense_rank[i]` is `1 +` the number of distinct smaller keys.

## Invariants

These must hold after every step, whatever the input:

- `rank[i] <= row_number[i]` and `dense_rank[i] <= rank[i]`.
- Equal keys have equal rank and equal dense rank.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Without ties all three are equal.
- `rank` jumps by the size of the tie group; `dense_rank` always by one.
- The last `dense_rank` is the number of distinct keys.

## Examples

Worked cases (the tests include them):

```text
keys [10,20,20,30]: row_number [1,2,3,4]; rank [1,2,2,4]; dense_rank [1,2,2,3]
```

## What the tests check

- The three on data with and without ties.
- A property against the definitions.

## Done when

All the `s3g_c4` tests pass, and you can say in one sentence what the bug was.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Modern SQL over in-memory tables](/cookbook/p/sql-window-functions-ctes): ROW_NUMBER, RANK and DENSE_RANK, and how peers share a rank.
- [Sort, top-N, limit, window](/cookbook/p/sort-top-n-limit-window): a window executor with peer groups and RANK.
