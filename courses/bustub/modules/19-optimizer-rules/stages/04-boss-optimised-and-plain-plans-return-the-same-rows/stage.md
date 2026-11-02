**Where this fits.** Three rules, each tested for the plan it produces. The one property that makes an optimizer an optimizer and not a bug generator is **equivalence**: the rewritten plan returns the same rows as the plan it came from. This stage checks that for queries that use several rules at once (a join on an equality, a filter, an order and a limit), with every rule on, with BusTub's starter rules only, and against a plain Rust computation, plus BusTub's own SQL files that check plans with `+ensure:`.

> [!CHECK] A rewrite passes all its plan-shape tests and still changes an answer on a table with NULLs and duplicate keys. Name two places in the three rules where that could happen, and the table you would build to expose each.
> ||(1) The hash join: duplicate keys on both sides (each left row must meet every right match) and NULL keys (they must match nothing, not each other): a table with `NULL, NULL, 1, 1` on each side. (2) The index scan: it keeps the whole predicate only if you pass it on; a lookup combined with a second condition on another column on rows where the second fails, and a lookup on a key that was deleted; or top-N when the sort has ties.||
>
> - Which plan is the oracle: the unoptimised one, or a Rust vector?
> - What is the smallest table that shows the difference?
> - Does the starter-rules run still use one of your rules?

## The task

Nothing new to write. Make both pass:

- **`stages_3h::s3h_04`**: for random tables `a(x, y)` and `b(p, q)` with NULLs and duplicates (distinct keys when an index on `b.p` exists), the query `select * from a join b on a.x = b.p where a.y > k order by a.x, a.y, b.p, b.q limit n` returns exactly the rows of a plain Rust computation with every optimizer rule on and with only BusTub's starter rules (`set force_optimizer_starter_rule=yes`: a nested index join where there is an index, no hash join, no top-N).
- **`sql_optimizer_test`** (`cargo test --test sql_optimizer_test`): BusTub's `p3.05-index-scan-btree`, `p3.06-empty-table`, `p3.14-hash-join`, `p3.15-multi-way-hash-join`, `p3.16-sort-limit` (with its `+ensure:hash_join` checks), `p3.17-topn` (with the heap-size check) and four small files (`hash_join`, `order_by`, `nested_index_join`, `update`) that issue `explain` and the query for each of several shapes.

## Your freedom

None new: a failure belongs to one of your rules (or to an executor of an earlier module).

## The Rust toolbox

**`explain (o)`** prints the optimized plan; `explain` alone prints every stage (bound, planned, optimized). Compare the plan text of the failing query with and without the starter flag to see which rule fired.

**Equivalence by two runs.** The helper runs a query, switches `force_optimizer_starter_rule`, runs it again and compares sorted rows; any difference is a rule changing an answer.

## If this is new

- Everything is in the earlier stages of this module.
- Y5 Testing & verification: Build it: equivalence: the optimised plan and the plain plan return the same rows, on random tables.

## Tests

- The equivalence property (all rules, starter rules, plain Rust).
- BusTub's seven `.slt` files with plan checks.

## Hints

### Plans that look right and answers that are not

Read the plan first (`explain (o)`), then the rows. If the plan has a `HashJoin` and the rows are wrong, the bug is in the rule's keys or in the executor of module 3f; if the plan is unchanged and the rows differ, it is an executor.

### Starter rules need unique keys

The nested index join (a starter rule, given) assumes the index holds one row per key, as a primary key does. The property gives the indexed table distinct keys for that reason.

## Performance

The `.slt` files run a few thousand rows; with the rules the joins use hashing and the whole file takes a second or two in debug mode. Without the hash join rule the multi-way joins take much longer: a vivid measure of what the rules buy.

## Experiment

Optional. Predict first, then run.

1. **Break one rule.** Make the hash join rule accept `a.x <= b.p`. Which of the two checks (plan shape, rows) notices first?
2. **Add a rule.** Write a rule that removes a filter whose predicate is the constant `true` (it exists in the given code as `optimize_eliminate_true_filter`): where in the rule order must it go, and what does the equivalence property say about it?

## Other designs

None for this stage. The *Other designs* sections of 3h-01 to 3h-03 list the alternatives to compare with yours.

## In BusTub

The rules live in `src/optimizer/` (`nlj_as_hash_join.cpp`, `sort_limit_as_topn.cpp`, `seqscan_as_indexscan.cpp`) and are stubs in Project 3; `optimizer_custom.cpp` chains them. `EXPLAIN` shows the plan before and after the optimizer.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `./bin/bustub-sqllogictest ../test/sql/p3.14-hash-join.slt --verbose` | `cargo test --test sql_optimizer_test p3_14` |
| `+ensure:hash_join` in the `.slt` | the same directive, checked by the Rust runner against `explain` |

**Port rule:** the same `.slt` files with plan assertions, run by the port.

## Learn more

- BusTub's [Project 3 page](https://15445.courses.cs.cmu.edu/fall2025/project3/) · *Testing database engines via pivoted query synthesis* and other equivalence-testing papers
