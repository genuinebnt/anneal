A town's meters report their **running totals**. A running total should never go down. When it does, a meter or a wire is at fault, and someone must find the first place where it happened.

You did not write the meters or the program that delivers their readings. You check what comes out.

## The data

A `Reading` says that a `meter` showed `total` at time `at`. Within a list, one meter's readings stand in the order they must be checked in. That order need not match `at`.

Two readings of the same meter, `before` and then `after`, **break** the rules if either holds:

- the total fell by **more than** `rules.tolerance`;
- the total stayed the same and `rules.allow_equal` is `false`.

A tolerance can be as large as `u64::MAX`.

## What to write

- `rules.strict()` returns the same rules, except that nothing may fall and nothing may stay equal.
- `by_meter(readings)` gives each meter its own readings, in list order. A meter without readings has no entry.
- `current_total(meters, meter, now, rules)` looks at the meter's **newest** reading: the largest `at` and, among those, the largest total. It returns that total if `at <= now` and `now - at <= rules.max_age`. Otherwise, or for an unknown meter, it returns `None`. An older reading never stands in for the newest one.
- `first_break(readings, rules)` gets **one** meter's readings. It returns the first two **neighbours** that break the rules.
- `trusted_start(day, rules)` gets a day's list with many meters mixed. It returns the longest start of the list in which no reading breaks the rules against the **previous reading of the same meter**.

## Days from a feed

A `Feed` gives the readings of one day, or a `FeedError`. The functions take it as `dyn Feed`, so they work with a feed they have never seen.

`Remembering::new(source)` is itself a `Feed`. It asks `source` for a day and keeps a successful answer. It never asks for that day again. An error is passed on and **not** kept.

`first_bad_day(feed, rules, days)` looks at the days **in order**. In a day, the bad reading is the **first in list order** that breaks the rules against the previous reading of its meter. Days are checked separately.

- It returns the first bad day with `before` and `after`, or `None`.
- It asks for no day after the first bad one. `days` may be `0..u64::MAX`.
- If the feed returns an error first, it returns that error at once.

## Size

A day may hold 200,000 readings of many meters. The tests run in a debug build.
