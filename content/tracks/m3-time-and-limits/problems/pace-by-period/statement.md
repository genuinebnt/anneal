A dashboard wants to know if this hour is busier than the last one. Waiting for the hour to end is too late. So it asks, at any moment, whether the work is arriving **faster per millisecond** than it did in the previous period.

All times are `u64` milliseconds, passed to you. Do not read any clock: the tests call you with whatever times they like, so `std::time` must not appear in your code.

Periods are fixed and aligned to zero. With `period_ms = 1000` they are `0..=999`, `1000..=1999`, and so on.

## What to write

`PaceMeter::new(period_ms)` (`period_ms >= 1`):

- `record(now_ms, units)` adds `units` to the period containing `now_ms`. A period's total stops at `u64::MAX`.
- `totals(now_ms)` returns `(previous, current)`. `previous` is the total of the period **directly before** the one containing `now_ms`. If nothing was recorded in it, it is 0.
- `pace(now_ms)` compares two rates. The previous rate is `previous / period_ms`. The current rate is `current / observed`, where `observed` counts the milliseconds of this period so far, **including** `now_ms`. At the first millisecond of a period, `observed` is 1.

`pace` returns `Faster`, `Slower` or `Level` (exactly equal). The comparison must be **exact** for every `u64` value.

A time earlier than one already passed to any method counts as that latest time.
