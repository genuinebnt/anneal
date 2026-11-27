A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`BloomFilter` in `src/primer/bloom.rs`: a bit array of `m` bits and `k` hash functions (derived from two base hashes by double hashing, `h1 + i * h2`). `insert(key)` sets `k` bits; `contains(key)` is true only if all `k` bits are set. There are **no false negatives**; false positives happen with a probability that depends on `m`, `k` and the number of keys. `union(other)` of two filters of the same shape contains everything either contained.

## Why

A Bloom filter is the smallest useful probabilistic structure: a few bits per key buy "definitely not here" answers that save a disk read (LSM trees check one per SSTable) or a network round trip. The part to get right is the one-sided error, and the part to understand is the cost: `m / n` bits per key buys an error rate you can compute.

## The contract

- `new(m_bits, k)`; `insert(&u64)`, `contains(&u64)`; `bits_set()` counts set bits; `union(&other) -> Option<BloomFilter>` (`None` if `m` or `k` differ).
- `positions(key)` (given) lists the `k` bit indexes of a key.

## Invariants

These must hold after every step, whatever the input:

- Every inserted key is contained, always.
- `bits_set() <= k * inserted_keys` and never more than `m`.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Inserting a key twice changes nothing.
- `union(a, b)` contains everything `a` or `b` contained, and its bits are the bitwise or.
- With 10 bits per key and 7 hashes, the false-positive rate on 1000 keys is below 3%.

## Examples

Worked cases (the tests include them):

```text
m 1000, k 3: insert 1..100 -> all contained; false positives among 1000..2000 are a few percent at most
```

## What the tests check

- No false negatives; duplicates.
- Union and its shape check.
- The measured false-positive rate against the theory.

## Done when

All the `s0d_c1` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Bloom filter](/cookbook/p/bloom-filter): a Bloom filter with k hashes by double hashing, and its false-positive rate.
- [Filters, tries, inverted and vector indexes](/cookbook/p/filters-tries-inverted-and-vector-indexes): Bloom, blocked Bloom and cuckoo filters: bits per key for 1% FPR.
