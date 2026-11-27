A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`hash_join` in `src/execution/hash_outer_join.rs`: the four join kinds (`Inner`, `Left`, `Right`, `Full`) on an equality of keys, computed with a hash table instead of a nested loop. Rows are `(Option<i64>, i64)`: a join key that may be NULL, and a payload. The answer is a list of `(left payload, right payload)` pairs where `None` is the NULL padding of an unmatched row.

## Why

The nested loop of stage 3j-01 does `|left| x |right|` comparisons and is the only choice for an arbitrary condition. For an equality it is the wrong tool: every real engine builds a hash table of one side and probes it with the other. An outer join adds the same bookkeeping as before (a flag per build row), and a NULL key adds the trap that a hash table happily stores a NULL as a key like any other while SQL says it matches nothing.

## The contract

- Two rows match when both keys are present and equal. A row with a NULL key matches nothing.
- `Inner`: every matching pair. `Left` adds each left row with no partner as `(Some(left), None)`. `Right` adds each right row with no partner as `(None, Some(right))`. `Full` adds both.
- The order of the answer does not matter; duplicates do (a row with three partners appears three times).
- The work is linear in the sizes of the inputs plus the size of the answer: 200 000 rows on each side must finish in a few seconds, which a nested loop cannot do.

## Invariants

These must hold after every step, whatever the input:

- The number of pairs with both sides present is the same for all four kinds.
- A left row appears in the answer at least once for `Left` and `Full`, and exactly as often as it has partners for `Inner` and `Right` (once, padded, if it has none and the kind keeps it).

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- `Left` = `Inner` + the unmatched left rows; `Right` = the pairs of `Left` with the sides swapped; `Full` = `Left` + the unmatched right rows.
- Swapping the two inputs and `Left` for `Right` gives the same pairs with the components swapped.

## Examples

Worked cases (the tests include them):

```text
left [(1,10),(2,20),(NULL,30)], right [(2,200),(3,300),(NULL,400)], Full -> (20,200) (10,None) (30,None) (None,300) (None,400)
duplicate keys: left [(1,1)], right [(1,5),(1,6)] -> (1,5) (1,6) for every kind
```

## What the tests check

- The four kinds against a nested-loop model on random inputs with NULLs and duplicates.
- The relations above.
- Empty inputs.
- A large input under a time limit.

## Done when

All the `s3j_c1` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Grace hash join and spilling](/cookbook/p/grace-hash-join-and-spilling): hash join with matched flags for LEFT joins and NULL keys never matching.
- [Join algorithms compared](/cookbook/p/join-algorithms-code): hash-based outer, semi and anti joins tested against nested loops.
