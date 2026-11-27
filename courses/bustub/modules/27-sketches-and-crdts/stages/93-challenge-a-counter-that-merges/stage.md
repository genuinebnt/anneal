A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`PnCounter` in `src/primer/pn_counter.rs`: a counter that many replicas can increment **and decrement** without coordination. Each replica keeps, per node, how much it has added and how much it has taken away; `value()` is total added minus total taken; `merge(&other)` combines two replicas by taking, for each node, the **larger** of the two counts on each side.

## Why

A plain counter shared by replicas loses updates (two replicas read 5 and both write 6). A grow-only counter per node cannot lose them but cannot decrement; a pair of them can. The reason merging works is three algebraic laws, and each is a test: merge in any order, any grouping, any number of times, and the replicas converge.

## The contract

- `PnCounter::new()`; `inc(node, n)`, `dec(node, n)` update that node's own counts; `value() -> i64`.
- `merge(&other)`: for every node, `p = max(p, other.p)` and `n = max(n, other.n)`.
- Nodes are `u32`.

## Invariants

These must hold after every step, whatever the input:

- For each node, the added and removed counts only ever grow.
- `value()` equals the sum of all added counts minus the sum of all removed counts.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Commutative: `a.merge(b)` and `b.merge(a)` give equal counters.
- Associative: merging in either grouping gives the same counter.
- Idempotent: merging a counter into itself, or merging twice, changes nothing.

## Examples

Worked cases (the tests include them):

```text
a: node 1 +5; b: node 2 +3, node 2 -1; merged value = 7; merged again = 7
```

## What the tests check

- Increments and decrements.
- Each law on examples.
- A property for random replicas.

## Done when

All the `s0d_c4` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [State-based CRDTs](/cookbook/p/crdts-code): a PN-counter as two G-counters, checked with law tests.
- [CRDTs](/cookbook/p/crdts): counters and other CRDTs whose merge converges without coordination.
