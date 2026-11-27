A challenge: no walkthrough, no hints, no solution. It is extra practice and does not count towards the course.

## What to fix

`src/container/hash/directory_split.rs` is the bookkeeping of an extendible hash directory: which bucket each slot points at, and how deep each bucket is. `split` splits a bucket, doubling the directory first when needed. It looks right, and it has one bug. Find it and fix it.

## Why

The directory is a few lines of index arithmetic, and a mistake in it does not crash: it silently sends some keys to the wrong bucket. The invariants of the structure are the only way to catch it, so writing them down is the exercise.

> [!CHECK] Once the tests pass: what was the bug, in one sentence, and why did ordinary inputs not show it?
> ||Doubling appended the existing slots in reverse order. After doubling, slot `i + 2^d` must point where slot `i` points (the new high bit is not used yet), so the new half is a copy in the same order.||

## The contract

- A directory of `2^global` slots; slot `i` is reached by the low `global` bits of a hash.
- A bucket of local depth `d` is pointed at by every slot that agrees with it on its low `d` bits.
- `split` raises the bucket's depth by one, creates a new bucket for the slots whose next bit is 1, and doubles the directory first (copying it) when the bucket is as deep as the directory.

## Invariants

These must hold after every step, whatever the input:

- Every local depth is at most the global depth.
- A bucket of depth `d` is pointed at by exactly `2^(global - d)` slots, which agree on their low `d` bits.
- Every slot points at exactly one bucket.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Doubling the directory never changes which bucket any hash value goes to.
- Splitting bucket `b` changes only the slots that pointed at `b`.
- Two hashes that agree on their low `d` bits of a bucket's depth `d` are always in the same bucket.

## Examples

Worked cases (the tests include them):

```text
new directory: 1 slot, 1 bucket, depth 0
split(0): global 1, 2 buckets, hash 0b10 -> bucket 0, 0b11 -> bucket 1
```

## What the tests check

- The first split and the doubling.
- Splitting a shallow bucket does not double.
- After a doubling, buckets are still reached by the slots that agree with them.
- A property over random sequences of splits checking every invariant.

## Done when

All the `s2b_c2` tests pass, and you can say in one sentence what the bug was.
