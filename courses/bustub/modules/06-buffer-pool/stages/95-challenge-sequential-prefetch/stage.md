A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`SeqDetector` in `src/buffer/seq_detector.rs`: it sees the page numbers a client reads, one by one, and returns the pages worth **prefetching**. After `trigger` consecutive increasing accesses (`p, p+1, p+2, ...`) it suggests the next `depth` pages; it never suggests a page it has already suggested in the current run, and any non-sequential access resets it.

## Why

A table scan reads page after page, and the disk is idle while the CPU waits for the next one. Reading ahead hides that latency, but only if the guess is right: prefetching on a random access wastes I/O and cache. A small state machine is all a pool needs to tell the two apart.

## The contract

- `access(page)` returns the pages to prefetch now (possibly none), in increasing order.
- A run is a sequence of accesses each exactly one more than the last. When the run's length reaches `trigger`, suggest `depth` pages after the current one; as the run goes on, suggest only pages not yet suggested.
- An access that is not `last + 1` starts a new run of length 1 (and forgets what was suggested).

## Invariants

These must hold after every step, whatever the input:

- A suggested page is always greater than the page just accessed.
- No page is suggested twice within one run.
- No suggestion is made before the run has `trigger` pages.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Two interleaved scans do not trigger a suggestion unless each is sequential on its own (this detector tracks one run).
- A scan of length `n >= trigger` ends with suggestions covering exactly up to `last + depth`.
- Repeating an access resets the run.

## Examples

Worked cases (the tests include them):

```text
trigger 3, depth 2: access 1, 2 -> none; 3 -> [4, 5]; 4 -> [6]; 5 -> [7]; 9 -> none
```

## What the tests check

- Trigger, depth, and the sliding window.
- Reset on a jump or a repeat.
- A property against a model of the run.

## Done when

All the `s1f_c4` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Buffered IO internals](/cookbook/p/buffered-io-internals): a prefetching reader that fetches the next block while the current one is used.
