A challenge: no walkthrough, no hints, no solution. It is extra practice and does not count towards the course.

## What to fix

`src/primer/hll_registers.rs` holds the registers of a HyperLogLog sketch and merges two sketches built over different parts of a stream, so that the merged sketch estimates the union. It looks right, and merging a sketch with itself changes it. Find the bug and fix it.

## Why

A sketch you can merge is a CRDT: the merged value must be the same however the data was split and however often the parts are combined. A merge that gets this wrong turns a sketch of the union into a number that grows every time two replicas talk.

> [!CHECK] Once the tests pass: what was the bug, in one sentence, and why did ordinary inputs not show it?
> ||Merge added the registers. A register is a maximum (the longest run of leading zeros seen), so merging takes the larger of each pair: `max` is idempotent, addition is not.||

## The contract

- `update(hash)` sets the register chosen by the top `bits` bits of the hash to `max(register, rank)` where `rank` is one more than the number of leading zeros of the remaining bits.
- `merge(&other)`: register-wise **maximum**, for sketches of the same size (else `Err`).

## Invariants

These must hold after every step, whatever the input:

- Every register is the largest rank ever seen for it.
- Registers never decrease.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- `merge(a, a) == a` (idempotent).
- `merge(a, b) == merge(b, a)` and `merge(merge(a, b), c) == merge(a, merge(b, c))`.
- The sketch of a stream equals the merge of the sketches of any split of it.

## Examples

Worked cases (the tests include them):

```text
a registers [1,0,3,0], b [0,2,3,0] -> merged [1,2,3,0]
```

## What the tests check

- Register-wise maximum.
- The three laws.
- Split-stream equality.

## Done when

All the `s0d_c5` tests pass, and you can say in one sentence what the bug was.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [HyperLogLog cardinality estimation from scratch](/cookbook/p/hyperloglog): HyperLogLog merge by register-wise max equals the sketch of the union.
- [State-based CRDTs](/cookbook/p/crdts-code): merge laws (commutative, associative, idempotent) as tests.
