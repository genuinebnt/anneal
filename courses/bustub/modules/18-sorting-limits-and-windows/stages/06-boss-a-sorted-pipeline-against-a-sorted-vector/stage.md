**Where this fits.** A sort, a limit, a top-N and a window function, each tested alone. Here one query goes through all of them: random rows are sorted (through a small buffer pool, so the sort goes to disk), cut by `LIMIT` (which the optimizer turns into a top-N), and ranked by a window, and every answer is compared with a plain Rust vector sorted with the same rules. Add BusTub's own SQL tests for sorting, limit, top-N and windows.

> [!CHECK] `select * from t order by a, b limit 5` can run as a sort followed by a limit or as a top-N, and `rank() over (order by a, b)` runs the sort again inside the window executor. The three must agree with each other. If the full sort and the top-N disagree on row 3, which two properties of your code would you test first, and with which input?
> ||(1) Ties: if the sort is stable and top-N is not (or the other way round), rows with equal keys swap places: test with all keys equal. (2) NULL ordering and direction: a comparator rule used by the merge but not by the heap, or the other way round: test with NULLs and `DESC NULLS FIRST`. A third suspect is the batch boundary: rows 127 and 128 of a batch.||
>
> - Which stage's code do all three share?
> - What does the stable sort promise about row 3 and row 4 when their keys are equal?
> - How do you shrink a failing table?

## The task

Nothing new to write. Make both pass:

- **`stages_3g::s3g_06`**: for random tables (NULLs, 400 rows, two keys with random directions and NULL rules) through a 24-frame pool: `order by` is the stable sort of the model; `order by .. limit n` is its first n rows; `rank() over (order by ..)` is, for every row, one plus the number of rows that sort strictly before it.
- **`sql_sort_and_window_test`** (`cargo test --test sql_sort_and_window_test`): BusTub's `p3.16-sort-limit`, `p3.17-topn`, `p3.19-integration-2` and `p3.20-window-function`. (The `+ensure:hash_join` checks need module 3h's optimizer rule; they are skipped here.)

## Your freedom

None new: a failure belongs to one of your stages.

## The Rust toolbox

**A sorted vector is the oracle.** The test sorts a `Vec` of `(row, position)` pairs with the comparator it spells out itself, then derives every expected answer from it; reading `model_compare` in the test file is the quickest way to learn what the rules are.

**Shrinking.** A failing run prints the smallest table and ordering that disagree; run the same SQL in the shell, put `explain` in front, and see whether it is a `Sort`, a `TopN` or a window.

## If this is new

- Everything is in the earlier stages of this module.
- Y5 Testing & verification: Build it: a sorted vector as the oracle for sort, limit, top-N and rank.

## Tests

- The pipeline property (sort, top-N, rank against a sorted vector).
- BusTub's four `.slt` files.

## Hints

### Three answers, one rule

If sort and top-N disagree, the bug is in how they use the comparator or break ties; if rank disagrees, it is in peer grouping; if only large tables fail, it is the merge or the pool.

## Performance

Four hundred rows sort in memory-sized runs of a page each; the property takes under a second. The `.slt` files sort a few thousand rows and run in a couple of seconds in debug mode.

## Experiment

Optional. Predict first, then run.

1. **Bigger and smaller pools.** Run the property with 12 and 200 frames. Does anything change in the results, and in the time?
2. **Break stability only in top-N.** Which test notices first?

## Other designs

None for this stage. The *Other designs* sections of 3g-01 to 3g-05 list the alternatives to compare with yours.

## In BusTub

`external_merge_sort_executor.cpp`, `limit_executor.cpp`, `topn_executor.cpp` and `window_function_executor.cpp` are stubs in Project 3 (`UNIMPLEMENTED("TODO(P3): Add implementation.")`). The 2025 version of the project asks for an external merge sort (`MergeSortRun`, `ExternalMergeSortExecutor<K>`), a top-N executor with a bounded heap, and window functions.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `./bin/bustub-sqllogictest ../test/sql/p3.16-sort-limit.slt --verbose` | `cargo test --test sql_sort_and_window_test p3_16` |

**Port rule:** the same `.slt` files, run by the port of the runner.

## Learn more

- BusTub's [Project 3 page](https://15445.courses.cs.cmu.edu/fall2025/project3/) · PostgreSQL's [EXPLAIN for sorts](https://www.postgresql.org/docs/current/using-explain.html)
