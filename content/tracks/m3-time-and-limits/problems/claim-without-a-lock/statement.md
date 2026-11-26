A buffer has `len` numbered units. Many threads take pieces of it, front to back, and nobody gives a piece back. Two threads must never get the same unit. At the end, one thread closes the buffer and ships exactly what was handed out.

You write the bookkeeping only: which range each caller gets.

## What to write

`Region::new(len)` has units `0..len`. It panics if `len > usize::MAX / 2`.

- `claim(n)` returns the next `n` units as a range, or `None` if fewer than `n` are left. A refused claim changes **nothing**, so a smaller one can still succeed. `claim(0)` gives an empty range.
- `claim_with(choose)` lets the caller size its piece. `choose(free)` is told how many units are free and answers how many to take. If it answers more than `free`, return `None`. (`claim` is already written in terms of this.)
- `free()` is the number of units that could still be claimed.
- `close()` returns how many units were handed out, and stops all further claims. Every call returns the same number. Afterwards every claim returns `None`, even `claim(0)`, and `free()` is 0.

## The guarantee

Everything must be right with any number of threads calling at once. Each successful call behaves as if it happened at **one single moment**:

- the range you return starts at the first unit free at that moment, and its length is what `choose` answered **for the number free at that moment**;
- if a claim succeeds, its units are included in the number `close` returns.

`choose` may be called more than once; only its last answer counts. It may also take a long time, and it may **call methods of the same region**. Your code must not hold a lock while calling it.

The tests use that last freedom. Inside `choose`, they claim from the region or close it. That puts "something happened in the middle" at an exact point, with no luck involved.
