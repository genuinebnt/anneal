A challenge: no walkthrough, no hints, no solution. It is extra practice and does not count towards the course.

## What to fix

`src/buffer/k_history.rs` keeps, for one page, the times of its last `k` accesses and answers the backward k-distance: how long ago the k-th most recent access was, or nothing if there have been fewer than `k`. It looks right, and it has one bug. Find it and fix it.

## Why

LRU-K lives on this one number, and an error of a single access in it is invisible in a demo and changes which page is evicted in production. Bugs like this are found by writing the definition down in the test as plainly as possible and comparing.

> [!CHECK] Once the tests pass: what was the bug, in one sentence, and why did ordinary inputs not show it?
> ||The history kept `k + 1` access times instead of `k`, so the access it reported as the k-th most recent was one access too old. An off-by-one in the trim condition: trim when the length exceeds `k`.||

## The contract

- After any number of accesses, `k_distance(now)` is `now` minus the time of the k-th most recent access, or `None` when fewer than `k` have been recorded.
- Only the last `k` accesses are remembered.

## Invariants

These must hold after every step, whatever the input:

- The history never holds more than `k` times.
- Times in the history are in increasing order.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- A more recent access never lengthens the distance.
- With `k = 1` the distance is the time since the last access.
- The distance at `now + d` is the distance at `now` plus `d`.

## Examples

Worked cases (the tests include them):

```text
k=2, accesses at 1, 5; now 10 -> 9
k=2, accesses at 1, 5, 9; now 10 -> 5
k=3, two accesses -> None
```

## What the tests check

- No distance before `k` accesses; the distance to the k-th most recent; `k = 1`.
- Only the last `k` accesses count.
- A property against the definition for any `k` and times.

## Done when

All the `s1d_c2` tests pass, and you can say in one sentence what the bug was.
