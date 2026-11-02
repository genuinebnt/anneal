Your engine has `JOIN` and `LEFT JOIN`. Ask it for `a RIGHT JOIN b` or `a FULL JOIN b` and the parser accepts the words and the planner builds the plan, but nothing can run it. The reason is worth a stage of its own. The nested loop join of module 3f takes one left row at a time and scans the right side for partners, so the moment the scan ends it knows whether *that left row* needs padding. Whether a *right* row went unmatched is only known after the last left row has been seen. This stage writes the executor that remembers.

> [!CHECK] `a` has the keys 1, 2, 2 and NULL; `b` has 2, 3 and NULL. For `a FULL JOIN b ON a.x = b.x`, write down every output row. Which rows are padded on which side, how many rows does the join return, and at which moment can your executor first say that `b`'s 3 is unmatched?
> ||Output: one pair (2, 2) for each of the two 2s in `a`; the rows 1 and NULL of `a` padded on the right; the rows 3 and NULL of `b` padded on the left. That is 2 + 2 + 2 = 6 rows. The two NULL keys do not match each other, so each side keeps its NULL row, padded. The 3 can only be declared unmatched after the **last** left row has been compared with it, so right-padded rows come out at the end, after the left side is exhausted.||
>
> - How is a RIGHT join different from a FULL join in what the executor does with a left row that found nothing?
> - If the same join runs as the inner side of another join, what must `init` do?
> - How many right rows does your executor hold at once? Is that a problem?

## The task

Implement `OuterJoinExecutor` in `src/execution/executors/outer_join_executor.rs` for `RIGHT` and `FULL` joins with any join condition (not only equalities). The executor factory already sends these plans to it.

- Every pair of rows that satisfies the condition is returned.
- A `FULL` join also returns each left row that matched nothing, with NULLs for the right columns.
- Both kinds return each right row that matched nothing, with NULLs for the left columns, after all left rows have been seen.
- A row whose join key is NULL matches nothing and is still returned padded.
- The executor can be initialised again and then produces the same rows, and it hands them out in batches of the size it is asked for.

## Your freedom

How the right side is stored and how you remember which rows matched (a flag per row, a set of indexes, a bitmap), whether you produce the output while reading the left side or queue it, and whether you hash the right side when the condition is an equality. The tests look at the rows, not at the order.

## The Rust toolbox

**`Vec<bool>` as a bitmap.** One flag per stored right row, set when a left row matches it. It is the simplest correct answer; a `HashSet<usize>` is the sparse version.

**`VecDeque` as an output queue.** One left row can produce many output rows but the caller asks for a fixed batch size, so produced rows wait in a queue until the caller takes them.

**Restartable state.** `init` must put every field back to its starting value, including the queue and the "left side finished" flag; the join may be the inner side of a nested loop that starts it again for each outer row.

## If this is new

- S2 Collections: `VecDeque`, `Vec<bool>`.
- Y5 Testing & verification: comparing an implementation with a model written in a few obvious lines.

## Tests

- RIGHT and FULL joins on small tables return the model's rows, including the padded ones.
- NULL keys on both sides match nothing and are kept.
- A left row with several partners produces one row per partner and no padded row.
- The condition may be `<`, or an equality with an extra term.
- Empty sides, and sides larger than a batch.
- Running the join twice (and as the inner side of another join) gives the same rows.
- A property over random tables with NULLs and duplicates, for LEFT, RIGHT and FULL.

## Hints

### Compare with a model that is obviously right

For each left row and each right row, evaluate the condition; track which rows matched. That is five lines in the test and the same five lines in your executor. If your executor is more clever (a hash table), keep this version around as the test's oracle.

### Count the rows first

`2 + 2 + 2 = 6` in the check above is the check you can do for every failing case: how many matched pairs, how many padded left rows, how many padded right rows. A wrong total tells you which of the three is missing.

### NULL keys come out of the predicate

You do not need a special case for NULLs: the condition's value for a NULL key is unknown, and only a value of **true** counts as a match. If you compared the key values yourself you would have to reproduce that rule.

## Performance

The nested loop does `|left| × |right|` comparisons and keeps the whole right side in memory, whatever the condition. That is the price of handling arbitrary conditions with one algorithm. For an equality condition a hash join does `|left| + |right|` work, and keeping a "matched" bit per build row gives the same padded rows.

**Measure it.** Join two 2 000-row tables with `a.x = b.x` and with `a.x < b.x`. Compare the times with `explain analyze` from module 3i and say which operator dominates.

## Experiment

Optional. Predict first, then run.

1. **Forget to reset the flags in `init`.** Run the FULL join as the inner side of another join. Which rows go missing?
2. **Use `==` on the key values instead of the condition.** Which test fails, and what does the row count of the NULL test show?

## Other designs

- **Hash outer join:** build a table on the right side, probe with the left, mark entries that were hit, and emit the unmarked ones at the end.
- **Sort-merge outer join:** both sides sorted on the key; unmatched rows are discovered while walking, with no extra memory.
- **Rewrite:** a RIGHT join is a LEFT join with the inputs swapped and the columns put back in order; a FULL join is a LEFT join plus the right rows that have no partner. Some systems implement only LEFT and rewrite the rest.

## In BusTub

BusTub's executors cover `INNER` and `LEFT` joins (project 3, task 2); `RIGHT` and `FULL` are not part of the project. This stage is an extension that exercises the same executor interface.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::vector<bool> matched(right.size())` | `vec![false; right.len()]` |
| `std::queue<Tuple>` for output not yet handed out | `VecDeque<Tuple>` |
| a flag that says "left side done" read in `Next` | an ordinary `bool` field; reset it in `init` |

**Port rule:** an operator that can be started again keeps all of its state in fields that `init` resets, never in function-local statics.

## Learn more

- [PostgreSQL: joined tables](https://www.postgresql.org/docs/current/queries-table-expressions.html#QUERIES-FROM) · [Rust: VecDeque](https://doc.rust-lang.org/std/collections/struct.VecDeque.html)
