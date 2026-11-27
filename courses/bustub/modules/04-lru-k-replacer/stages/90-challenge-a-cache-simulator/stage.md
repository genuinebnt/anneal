A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

Two functions in `src/buffer/cache_sim.rs`. `simulate` replays a trace of page accesses through a pool of a fixed number of frames, with any `FrameReplacer` choosing victims, and returns the number of hits. `belady_hits` returns the best any policy could do on the trace: Belady's rule evicts the page whose next use is furthest in the future.

## Why

"Which policy is better?" has no answer without a workload. A simulator turns the question into a number, and the optimum tells you how much room is left. This is how real systems are tuned: from traces of production access patterns.

## The contract

- A miss with a free frame uses it; a miss with the pool full evicts the replacer's victim and reuses its frame. The replacer sees every access, and every page is evictable as soon as it is used. A capacity of 0 has no hits.
- `belady_hits` may look at the whole trace.

## Invariants

These must hold after every step, whatever the input:

- `0 <= hits <= len(trace) - distinct pages used`, at least the first use of each page misses.
- Every page in the pool is a page of the trace.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- No policy has more hits than `belady_hits`.
- A bigger pool never has fewer hits under the optimal policy.
- With capacity at least the number of distinct pages, hits = len - distinct.

## Examples

Worked cases (the tests include them):

```text
[1,2,3,1,2,3,1,2,3], capacity 3 -> 6 hits
[1,1,2,2,2,1,3], capacity 1 -> 3 hits
[1,2,3,1,2], capacity 2: optimal 1 hit; FIFO 0 hits
```

## What the tests check

- Small traces worked by hand.
- A hot page between a scan, where frequency beats recency.
- No policy beats the optimum; a bigger pool never hurts it.
- LRU through the simulator equals a one-line model of LRU.

## Done when

All the `s1d_c1` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [A cache replacement policy zoo](/cookbook/p/cache-policy-zoo): replaying traces through many policies, with Belady's OPT as the bound.
- [Buffer pools](/cookbook/p/buffer-pools): a lab that replays traces to watch LRU lose to sequential flooding.
