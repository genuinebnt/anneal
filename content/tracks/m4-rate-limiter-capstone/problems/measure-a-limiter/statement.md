> **Spoiler before you open the editor.** This is step 1, so the starter has no earlier steps. It does include a working `SlidingLog`.
>
> That solves the sliding window of series part 3, "A rate limiter on a clock you control". Solve it first. You may replace the given code, but keep the public names.


**Capstone, step 1 of 7.** You are building a laboratory that compares rate limiters on the same traffic. First it needs a way to run any limiter and measure it.

All times are `u64` milliseconds, passed in. `std::time` must not appear in your code.

`Limiter::check(now_ms)` answers whether one request arriving then may pass. An earlier time than the limiter has seen counts as the latest one.

## What to write

**`FixedWindow::new(limit, window_ms)`** cuts time into windows aligned to zero: `0..window_ms`, `window_ms..2 * window_ms`, and so on. In each window it admits the first `limit` requests and refuses the rest.

**`replay(limiter, arrivals, span_ms)`** offers the arrivals to the limiter one by one, calling `check` exactly once for each. The list is in the order the requests reach you. A time earlier than one before it counts as that latest time, and the limiter is given the latest time.

It returns a `Report`: every decision, the numbers allowed and denied, and `busiest`. `busiest` is the most **admitted** requests whose times lie in one half-open stretch `s <= t < s + span_ms`, for any `s`. It uses the effective times.

`replay` must work for `&mut dyn Limiter` too. There are up to 200,000 arrivals.
