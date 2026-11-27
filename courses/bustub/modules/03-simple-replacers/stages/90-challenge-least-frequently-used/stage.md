A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`LfuReplacer` in `src/buffer/lfu_replacer.rs`: a replacement policy that evicts the frame used the **fewest times**; among equal counts the one whose last use is oldest goes first. It plugs in wherever a `FrameReplacer` does.

## Why

LRU asks "which page was used longest ago?"; LFU asks "which is used least?". They disagree exactly where it matters: a long scan of pages each used once pushes a hot page out of an LRU cache, while LFU keeps it.

## The contract

- A frame met for the first time starts with one access and is **not** evictable; every `record_access` adds one.
- `evict` removes and returns the evictable frame with the fewest accesses (oldest last access among equals), or `None`; an evicted or removed frame is forgotten, count included.
- `remove` forgets an evictable frame; a known frame that is not evictable panics. `size` counts evictable frames; unknown frames are ignored.

## Invariants

These must hold after every step, whatever the input:

- `size()` equals the number of frames that are known and evictable.
- A victim is always an evictable frame.
- An evicted frame is no longer known: using it again starts from one access.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Adding an access to a frame never makes it more likely to be evicted before the others.
- Pinning a frame (making it not evictable) never changes any other frame's order.

## Examples

Worked cases (the tests include them):

```text
accesses: 0,1,2,0,0,1; all evictable -> evict order 2, 1, 0
tie on one access: the one used first goes first
```

## What the tests check

- Eviction order by count and the tie-break.
- New frames are not evictable; pinning again works; forgotten frames restart.
- `remove` and its panic.
- A property against a plain model of the policy.

## Done when

All the `s1c_c1` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [A cache replacement policy zoo](/cookbook/p/cache-policy-zoo): LFU next to FIFO, LRU, ARC and others, replayed on the same traces.
- [Problems that need several data structures working together](/cookbook/p/combo-algorithms): an O(1) LFU cache built from frequency buckets of linked lists.
