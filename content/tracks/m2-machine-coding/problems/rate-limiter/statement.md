**Interview prompt. Time box: 120 minutes** (core in about 35). The tests are named `core_`, `part2_`, `part3_` and
`part4_`. The problem counts as solved when all parts pass.

Build the rate limiting for an API. **Time comes only from the `Clock` you are given.** The tests move a fake clock by
hand and never sleep, so `std::time` must not appear in your code. A clock may report an **earlier** time than before
(a correction); treat that as no time passing, and never give the same stretch of time twice.

All times are `u64` milliseconds. Any `u64` may be reported, including very large ones.

## Core: a token bucket

`TokenBucket::new(clock, capacity, refill_every_ms)` (`refill_every_ms >= 1`):

- it starts **full**, with `capacity` tokens;
- one token is earned every `refill_every_ms` milliseconds. Progress towards the next token is never lost, however
  often you are called;
- a bucket never holds more than `capacity` tokens, and **while it is full no progress accumulates**;
- `try_acquire(n)` takes `n` tokens if all of them are there and returns `true`; otherwise it takes nothing and returns
  `false`. `try_acquire(0)` is always `true`.

## Part 2: when to come back

`retry_after_ms(n)` takes nothing. It returns `Some(0)` if `try_acquire(n)` would succeed now, otherwise the smallest
`d` such that it succeeds if the clock advances by exactly `d` and nothing else is taken, or `None` if it never can.
A result that does not fit in a `u64` is `u64::MAX`.

## Part 3: a second policy behind one interface

- Implement the given trait `Limiter` for `TokenBucket`: `allow()` is one token.
- `SlidingWindow::new(clock, limit, window_ms)` (`window_ms >= 1`) implements it too: `allow()` admits a request if
  **fewer than `limit` requests were admitted in the last `window_ms` milliseconds**. A request admitted at time `t`
  counts while `now < t + window_ms`. Rejected requests do not count. Its memory must stay proportional to `limit`.

## Part 4: one limiter per client

`PerKey::new(clock, idle_ms, make)` (`idle_ms >= 1`) gives every key its own limiter, made by `make()` when the key
is first seen. `allow(&key)` asks that key's limiter. A key whose last call was **`idle_ms` or more** before the
current time is forgotten, and gets a new limiter if it returns. `tracked(&mut self)` returns how many keys are
remembered **now**, after forgetting the idle ones.

There may be hundreds of thousands of keys: a call must not take time proportional to the number of keys (forgetting
may be amortised).
