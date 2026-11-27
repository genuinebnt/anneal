A challenge: no walkthrough, no hints, no solution. It is extra practice and does not count towards the course.

## What to fix

`grantable` in `src/concurrency/grant.rs` decides which waiting requests of one lock may be granted **now**, given the modes of the current holders and the waiting queue in arrival order. The rule is fair: requests are considered in order and the scan **stops** at the first one that cannot be granted. It looks right, and a reader gets in past a waiting writer. Find the bug and fix it.

## Why

Readers keep arriving, each compatible with the current readers, and a rule that lets them in can keep the writer at the head of the queue waiting forever. That is starvation, and the loop that causes it still produces plausible answers.

> [!CHECK] Once the tests pass: what was the bug, in one sentence, and why did ordinary inputs not show it?
> ||On a request it could not grant, the scan used `continue` and went on, so later compatible readers were granted ahead of the waiting writer. Fairness is `break`: stop at the first request that cannot be granted.||

## The contract

- `grantable(holders, waiting)` returns the indexes (into `waiting`) of the requests to grant now, in order.
- A request is granted if it is compatible with every holder **and** with every request granted before it in this call; the scan stops at the first request that is not granted.

## Invariants

These must hold after every step, whatever the input:

- The granted set is always a prefix of the waiting queue.
- Granted requests are pairwise compatible and compatible with all holders.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- If the first waiter is incompatible with a holder, nothing is granted.
- Adding a waiter at the end never changes which earlier waiters are granted.
- With no holders, the first waiter is always granted.

## Examples

Worked cases (the tests include them):

```text
holders [S]; waiting [X, S] -> none (the reader may not pass the writer)
holders []; waiting [S, S, X, S] -> [0, 1]
```

## What the tests check

- Prefix property and the stop.
- Compatible runs.
- A property: the granted set is a compatible prefix.

## Done when

All the `s4d_c4` tests pass, and you can say in one sentence what the bug was.
