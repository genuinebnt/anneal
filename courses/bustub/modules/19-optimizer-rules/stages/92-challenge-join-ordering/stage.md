A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`best_join_order` in `src/optimizer/join_order.rs`: given the row counts of `n` tables (`n <= 12`) and a symmetric matrix of join selectivities, find the **left-deep** order (`((t1 ⋈ t2) ⋈ t3) ...`) that minimises the **sum of the sizes of the intermediate results**. The size of joining a set `S` of tables is the product of their row counts times the selectivity of every pair inside `S`.

## Why

The order of joins is the biggest lever a query optimiser has: the same query can cost a thousand times more in a bad order. The number of orders grows factorially, but the *cost of a set* does not depend on how it was assembled, which is exactly what makes dynamic programming over subsets work. This is the System R algorithm.

## The contract

- `card[i]` is the row count of table `i`; `sel[i][j] = sel[j][i]` is the selectivity of joining `i` with `j` (1.0 = no predicate, a cross product).
- `size(S) = prod(card[i] for i in S) * prod(sel[i][j] for pairs i < j in S)`.
- The cost of an order `p0, p1, ...` is `size({p0, p1}) + size({p0, p1, p2}) + ...` (the base tables themselves are not counted). One table has cost 0.
- Return `(cost, order)` for a cheapest order; on ties the lexicographically smallest order.

## Invariants

These must hold after every step, whatever the input:

- The returned order is a permutation of `0..n`.
- The cost equals the cost of the returned order.
- No order is cheaper (checked by brute force for small `n`).

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Making a selectivity smaller never increases the best cost.
- Relabelling the tables relabels the answer: the best cost is unchanged.
- With all selectivities 1.0, the best order joins the smallest tables first.

## Examples

Worked cases (the tests include them):

```text
card [1000, 10, 100], sel(0,1) = 0.001: join 0,1 first (size 10) then 2 (size 1000)
```

## What the tests check

- Small cases worked by hand.
- All selectivities 1.0.
- A property against brute force over all permutations for n <= 6.

## Done when

All the `s3h_c3` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Cost models and join ordering](/cookbook/p/cost-models-and-join-ordering): System R dynamic programming over left-deep orders with subset bitmasks and C_out.
- [Join ordering](/cookbook/p/join-ordering): DPsize, DPsub and left-deep DP compared on the C_out cost.
- [Join enumeration](/cookbook/p/join-enumeration-dpccp-dphyp): DPsub over subsets, and faster enumerators that give the same optimum.
