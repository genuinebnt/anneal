The simplest correct join: for every row of the **left** child, scan the whole **right** child and output a combined row for every pair on which the **join predicate** is TRUE. It works for any predicate (`a.x < b.y`, `a.x + 1 = b.y`, no predicate at all for a cross join), which is why it is also what the planner emits first; the faster joins of the next stages work only for equalities. A **left join** also outputs each left row that matched *nothing*, once, with NULLs on the right.

> [!CHECK] The right child is scanned once per left row. How does the executor get a fresh scan of the right child without building a new executor, and what is the cost for a left table of 1 000 rows and a right table of 1 000 rows? Then: in a left join, a left row's predicate answers are `NULL, FALSE, NULL`: is the row padded with NULLs, and why?
> ||`init` on the right child rewinds it, so the executor calls `right.init()` for every left row; the cost is 1 000 × 1 000 pair evaluations, a million, which is why this join is the baseline to beat. Yes, it is padded: a pair matches only when the predicate is TRUE; FALSE and NULL (unknown) are both "no match", so the left row matched nothing.||
>
> - Which side is the outer (left) loop in the output order?
> - What if one left row has more matches than a batch holds?
> - Where do you keep the left row between two `next` calls?

## The task

In `src/execution/executors/nested_loop_join_executor.rs` (the struct, `new`, the `TupleStream` wrappers around the children, `values_of` and `joined` are given):

- `init`: initialise both children and forget the current left tuple, the `matched` flag and any pending output.
- `next`: until the batch is full: hand out `pending` output first; if there is no current left tuple, take the next from the left (none: stop) and **re-initialise the right**; take the next right tuple and, if the predicate's `evaluate_join` is TRUE for the pair, queue the joined tuple and set `matched`; when the right side is exhausted, queue `unmatched_output(..)` if it returns something and drop the left tuple. Return `true` if the batch is not empty.
- `unmatched_output(left) -> Option<Tuple>`: for a **LEFT** join and a left tuple for which `matched` is false, `Some` tuple of the left values followed by a NULL of the right column's type for every right column (`values_of`, `nulls_for`); otherwise `None`.

The tests: exact scenarios (an inner join outputs the matching pairs; the predicate can be any expression; NULL never matches; a cross join has every pair; one left row with more matches than a batch; a left join pads unmatched rows once and with typed NULLs; the join works over non-table children), and a property: **for random tables with NULLs and random predicates** (any comparison of sums and differences of the four columns, combined with `and`/`or`) the inner join returns exactly the pairs the predicate keeps and the left join also the unmatched left rows, padded.

## Your freedom

Where you hold the current left row, whether you queue output in a buffer (`pending`, given) or return as soon as the batch fills, and how you pull from the children.

## The Rust toolbox

**`evaluate_join`.** `self.predicate.evaluate_join(left, left_schema, right, right_schema)?` takes the two tuples with their schemas (module 3d); `.as_bool() == Some(true)` is the match test.

**State between calls.** `current_left: Option<Tuple>`, `matched: bool` and `pending: VecDeque<Tuple>` are fields: a `next` call may stop in the middle of a left row because the batch is full.

**Moving out of an `Option` field.** `self.current_left.take()` leaves `None` behind and gives you the value; `self.current_left.as_ref()` borrows it.

**A wrapper that pulls one tuple at a time.** `TupleStream` (given) turns a batched child into `next_tuple() -> Result<Option<Tuple>>` so you write the loop as in the textbook.

**Typed NULLs.** `Value::null(column.type_id())`; `nulls_for(schema)` (given) builds the padding.

## If this is new

- [S1 Option & Result](/t/s1-option-result): `take`, `as_ref`, `?` in nested loops.
- S5 Queues & heaps: `VecDeque` as a buffer.
- [L3 Lifetimes](/t/l3-lifetimes): executors that own their children.
- The optional *join algorithms* concept.
- S6 Iterators: Understand: pulling from iterators by hand; a cursor over a `Vec`.
- Y5 Testing & verification: Build it: differential tests: three algorithms against each other and a naive one.

## Tests

- Inner join: matching pairs; any predicate; NULL never matches; cross join; a left row with more matches than a batch; the output order.
- Left join: padded once, typed NULLs; unmatched when the answers are NULL or FALSE; works through `init` again.
- Property: random tables and predicates against every pair checked.

## Hints

### State across batches

If a left row has 200 matches and the batch is 128, `next` must return with the left row still current and `matched` still true; the next call continues with the right side where it was.

### The padded row is a decision at the end

Only after the right side is exhausted do you know that nothing matched. `unmatched_output` is called exactly there.

## Performance

`n × m` predicate evaluations, each a tree walk: for two tables of 10 000 rows that is a hundred million evaluations, about ten seconds. This is the reason the optimizer (module 3h) turns an equality join into a hash join.

**Measure it.** Join two tables of 1 000, 2 000 and 4 000 rows on an equality with this executor; the time should quadruple when both sizes double.

## Experiment

Optional. Predict first, then run.

1. **Block nested loops.** Read a batch of left rows and scan the right side once per *batch*. How much faster, and what changes in the output order?
2. **Swap the sides.** Put the smaller table on the left. Does the cost change?

## Other designs

- **Tuple-at-a-time nested loops (ours).**
- **Block nested loops:** one scan of the inner per block of outer rows.
- **Index nested loops:** stage 5.
- **Sort-merge join:** both inputs sorted by the key.

## In BusTub

`aggregation_executor.cpp`, `nested_loop_join_executor.cpp`, `hash_join_executor.cpp` and `nested_index_join_executor.cpp` are stubs in Project 3 (`UNIMPLEMENTED("TODO(P3): Add implementation.")`); the header comments carry the contract. The executors are batched, as in module 3e.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `predicate_->EvaluateJoin(&left, left_schema, &right, right_schema)` returning a `Value` | `predicate.evaluate_join(..)?` returning a `Value` |
| `ValueFactory::GetNullValueByType(type)` | `Value::null(type_id)` |
| `bool left_matched_` member | `matched: bool` |

**Port rule:** loop state kept in members stays in fields; the loop is rewritten as a resumable `next`.

## Learn more

- PostgreSQL's [join strategies](https://www.postgresql.org/docs/current/planner-optimizer.html) · module 3d's `evaluate_join`
