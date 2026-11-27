A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`plan_sort` in `src/execution/sort_plan.rs`: given `pages` (the size of the input) and `buffer` (pages of memory), return how an external merge sort would go: the number of initial sorted runs (`ceil(pages / buffer)`), the number of **passes** (pass 0 makes the runs; each merge pass merges up to `buffer - 1` runs at a time), and the total pages read plus written (`2 * pages * passes`). `None` if `buffer < 3` (nothing can be merged) or `pages == 0` has zero passes.

## Why

A query optimiser must know what a sort will cost before it chooses a plan, and the formula `1 + ceil(log_{B-1}(ceil(N/B)))` is easy to get wrong with floating point (`log` of an exact power of the base rounds the wrong way and costs an extra pass). Counting passes by simulation is exact and shows what the algorithm does.

## The contract

- `runs0 = ceil(pages / buffer)`.
- `passes` = 1 (the run-creation pass) plus the number of merge passes: repeatedly replace `r` runs by `ceil(r / (buffer - 1))` until one run is left.
- `io_pages = 2 * pages * passes`. For `pages == 0`: no runs, zero passes, zero I/O.

## Invariants

These must hold after every step, whatever the input:

- The last merge pass ends with exactly one run.
- `passes >= 1` for any non-empty input.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- More memory never needs more passes.
- A larger input never needs fewer passes.
- If `pages <= buffer`, there is one pass (the sort happens in memory).

## Examples

Worked cases (the tests include them):

```text
pages 100, buffer 10: runs 10, merge fan-in 9 -> 10 -> 2 -> 1: passes 1 + 3 = 4, io 800
pages 8, buffer 10: passes 1
```

## What the tests check

- Exact plans for small cases and for powers of the fan-in.
- Monotonicity.
- A property against a simulation.

## Done when

All the `s3g_c1` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [External sort hash aggregation](/cookbook/p/external-sort-hash-aggregation): external merge sort with passes and IO asserted against the formula.
- [Sorting and aggregation](/cookbook/p/sorting-and-aggregation): the external sort cost formula, verified on runs of different sizes.
