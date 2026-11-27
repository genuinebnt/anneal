A radio station wants a chart of what is hot **right now**. A song played a lot last month should slide down, even if nobody plays anything else today.

**The score.** Each play fades with time. After `half_life` seconds it counts half, after two half-lives a quarter, and so on. At time `now`, one report of `plays` plays made at time `at` is worth

`plays * 0.5 ^ ((now - at) / half_life)`

The division is not rounded. A song's **score** is the sum over its reports.

`Chart` has:

- `new(half_life)`, with `1 <= half_life <= 1 000 000 000`.
- `report(song, plays, at)` adds a report. Reports may arrive in **any order of `at`**.
- `score(song, now)` is the song's score at `now`. An unknown song scores `0.0`.
- `top(n)` returns the names of at most `n` songs, hottest first. Only songs with at least one play are listed.

Times are whole seconds from 0 to 1 000 000 000. In `score(song, now)`, `now` is never before any report. A station runs for years, so `now / half_life` may be a billion.

**Accuracy.** A score must be within one part in 100 000 (`1e-5`, relative) of the exact value. A score too small for an `f64` may be `0.0`: any exact value below `1e-300` accepts any answer from 0 to `1e-300`.

A score is never infinite and never NaN. One test sends at most 2 000 reports for one song.

**Order.** `top` compares scores at one common moment, so the order does not depend on when you ask.

- If two scores differ by more than one part in 100 000, the higher comes first. This also holds when both are too small to show as an `f64`.
- Two songs that got exactly the same reports, in the same order, **tie**. Tied songs are listed by name, smallest first.
- Other songs whose scores are closer than that may come in either order.

Speed and memory are not graded.
