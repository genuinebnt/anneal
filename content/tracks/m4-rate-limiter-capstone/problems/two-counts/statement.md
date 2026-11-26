> **Spoiler before you open the editor.** The starter contains reference code for step 1, and a working `SlidingLog`. It is a stand-in until the app can carry your own code from step to step.
>
> It solves step 1, the sliding window of series part 3, "A rate limiter on a clock you control", and the main idea of part 2, "The busiest stretch". Its fixed window also shows the period roll of part 1. Solve those first. You may replace the given code, but keep the public names.


**Capstone, step 2 of 7.** The fixed window lets twice the limit through at an edge. The log fixes that, but stores every admitted time. This limiter sits between them, with two counters.

Windows are aligned as in step 1. Say the time is `offset` milliseconds into its window. Then a part of the previous window, `window - offset` milliseconds of it, still lies within the last `window` milliseconds.

The limiter **estimates** the count in the last `window` milliseconds as:

`previous × (window − offset) / window + current`

Here `previous` and `current` count the **admitted** requests of the previous window and of this one.

## What to write

**`SlidingCounter::new(limit, window_ms)`** and its `check`: admit a request when `estimate + 1 <= limit`. The test must be **exact**. No rounding, for any `u32` limit and `u64` window.

**`retry_after_ms(now_ms)`** counts nothing. It returns `Some(0)` if a request would pass now. Otherwise it returns the smallest `d` such that a request at `now_ms + d` passes, with no other requests in between. It returns `None` if no `u64` time would do.

An earlier time counts as the latest one seen, by either method. `d` is measured from that latest time. A window can be billions of milliseconds long, so the wait cannot be found by trying them one by one.
