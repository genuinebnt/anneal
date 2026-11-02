**Where this fits.** Three join algorithms and an aggregation, each tested alone. This stage runs them **against each other**: the same equality join through a nested loop, a hash table and an index must return the same rows, and the same rows as checking every pair. That kind of test is called **differential**: no expected answer is written down, any disagreement is a bug in at least one of the implementations. Add BusTub's own SQL tests for the same executors.

> [!CHECK] Three correct join algorithms must produce the same *multiset* of rows but not the same *order*. What does a test compare, and what property of an inner join lets you check it a fourth way without writing a new algorithm? If the three disagree on a case, how do you decide which one is wrong?
> ||Compare sorted rows (the order is not part of the contract). An inner join commutes: `l JOIN r` gives the same pairs as `r JOIN l` with the columns swapped, so running it with the tables swapped is a free fourth check. To find the culprit, write the case down as the smallest tables that still disagree, compute the answer by hand (every pair), and see which algorithm differs: usually it is the one that treats NULL keys, duplicate keys or padding differently.||
>
> - Which algorithm is the oracle?
> - What is the smallest pair of tables that makes two of them disagree?
> - Does a left join commute?

## The task

Nothing new to write. Make both pass:

- **`stages_3f::s3f_06`**: for random tables `l` (duplicate keys, NULLs) and `r` (unique keys, an index on the key), the join `l [left] join r on l.k = r.k` through the nested loop join (plain SQL), the hash join (a hand-built plan) and the nested index join (`set force_optimizer_starter_rule=yes`) returns the rows of the naive join; an inner join with the tables swapped returns the same pairs; and `count(*)` over the join equals the number of those rows.
- **`sql_aggregation_and_joins_test`** (`cargo test --test sql_aggregation_and_joins_test`): BusTub's `p3.07-simple-agg`, `p3.08-group-agg-1`, `p3.09-group-agg-2`, `p3.10-simple-join`, `p3.11-multi-way-join`, `p3.12-repeat-execute` and `p3.13-nested-index-join`. (`p3.14` and `p3.15`, the hash join files, wait for module 3h's optimizer rule.)

## Your freedom

None new: a failure belongs to one of your executors.

## The Rust toolbox

**Shrunk counterexamples are tables.** A failing run prints the two tables (shrunk to a handful of rows). Create them in the shell and run the three joins by hand with `explain` to see which plan runs.

**Switching the algorithm.** No flag: nested loop; `set force_optimizer_starter_rule=yes` with an index: nested index join; a hand-built `HashJoin` plan (as in stage 4) for the hash join.

## If this is new

- Everything is in the earlier stages of this module.
- Y5 Testing & verification: Build it: differential tests: three algorithms against each other and a naive one.

## Tests

- The differential property over random tables.
- BusTub's seven `.slt` files.

## Hints

### Two of three agree

The odd one out is wrong. If the nested loop disagrees with the other two, check NULL keys (`NULL = NULL` is not true) and left-join padding; if the hash join, check duplicate keys and the key equality; if the index join, check deleted rows and NULL outer keys.

## Performance

The slt files include a few thousand rows; the joins use the nested loop where there is no index, so the whole file takes a few seconds in debug mode.

## Experiment

Optional. Predict first, then run.

1. **Larger tables.** Raise the sizes in the property to 200 rows. How long does each algorithm take? Which is the first to become slow?
2. **A fourth algorithm.** Write a sort-merge join as a plan node and add it to the differential test.

## Other designs

None for this stage. The *Other designs* sections of 3f-01 to 3f-05 list the alternatives to compare with yours.

## In BusTub

`aggregation_executor.cpp`, `nested_loop_join_executor.cpp`, `hash_join_executor.cpp` and `nested_index_join_executor.cpp` are stubs in Project 3 (`UNIMPLEMENTED("TODO(P3): Add implementation.")`); the header comments carry the contract. The executors are batched, as in module 3e.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `./bin/bustub-sqllogictest ../test/sql/p3.10-simple-join.slt --verbose` | `cargo test --test sql_aggregation_and_joins_test p3_10_simple_join` |

**Port rule:** the same `.slt` files, run by the port of the runner.

## Learn more

- BusTub's [Project 3 page](https://15445.courses.cs.cmu.edu/fall2025/project3/) · *Differential testing* (McKeeman, 1998)
