> **Spoiler before you open the editor.** The starter contains reference code for step 5, the gate. It has nothing from steps 1 to 4. It is a stand-in until the app can carry your own code from step to step.
>
> It solves step 5 and the main idea of series part 8, "Hand out space that nobody else has". Its permit also shows the guard of part 5. Solve those first. You may replace the given code, but keep the public names.


**Capstone, step 6 of 7.** A fixed cap is a guess, and the right number changes. Now the gate learns it from the work itself. Each admitted piece of work gets a ticket, and the ticket reports how it went.

## What to write

`Adaptive::new(initial, min, max)` panics unless `1 <= min <= max`. The first limit is `initial`, moved into `min..=max`. Clones share one `Adaptive`.

`try_acquire()` gives a ticket if fewer than `limit()` are out. Each ticket ends in one of three ways, and in every case its place is free afterwards:

- **`success()`** raises the limit by **1**, up to `max`. This happens only if the ticket **took the last free place** when it was given. A gate that is never full has not shown that it needs more room.
- **`overload()`** cuts the limit to **half**, rounded down, but not below `min`.
- **Dropped without a report** (cancelled work, a client's own mistake): the limit does not change.

**One signal per episode.** When 50 requests time out together, that is one overload, not 50. So after a cut, a ticket given **before** that cut changes nothing, whether it reports success or overload. A cut that leaves the limit at `min` is still a cut.

All of this must hold when tickets are reported from many threads at once.
