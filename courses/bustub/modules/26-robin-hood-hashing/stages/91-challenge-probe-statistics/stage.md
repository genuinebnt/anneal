A challenge: no walkthrough, no hints, no solution. It adds to something you built in this module, using what you learned there. It is extra practice and does not count towards the course.

## What to build

`probe_histogram` and `mean_probe` in `src/primer/probe_stats.rs`: using the public view of **your** `RobinHoodHashSet<i32>` (`home_bucket`, `get_bucket`, `probe_distance`), compute how far each of a list of keys sits from its home bucket: `probe_histogram(set, keys)[d]` is the number of keys at distance `d`, and `mean_probe` the average.

## Why

The point of Robin Hood hashing is a table where the *worst* probe distance stays small: rich keys give way to poor ones, so the variance of distances is low. You cannot see that from the answers (they are the same as any set's); you see it by measuring. A histogram of distances is the standard tool, and it is also how you find a bad hash function.

## The contract

- `probe_histogram(set, keys)`: for each key found in the set, its distance; the result has one entry per distance from 0 to the maximum, and counts the keys (absent keys are skipped).
- `mean_probe(set, keys)`: the mean distance of the found keys (0.0 if none).

## Invariants

These must hold after every step, whatever the input:

- The counts sum to the number of found keys.
- The largest index with a non-zero count is at most `max_probe_distance()` of the set.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- On a table at most half full, most keys are at distance 0 or 1.
- Removing keys never raises the maximum distance of the remaining ones (Robin Hood property, tested on your implementation).
- The histogram of a key list with duplicates counts each occurrence.

## Examples

Worked cases (the tests include them):

```text
capacity 16, keys 1..8: histogram sums to 8
```

## What the tests check

- Counts and mean on a small table.
- Absent keys.
- Your table's maximum distance against the histogram.

## Done when

All the `s0c_c2` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Open addressing map](/cookbook/p/open-addressing-map): Robin Hood displacement and the probe distances it equalizes.
