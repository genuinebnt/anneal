> **Spoiler before you open the editor.** The starter contains reference code for steps 1 to 3, with a working `SlidingLog` and `TokenBucket`. It is a stand-in until the app can carry your own code from step to step.
>
> It solves those steps and both limiters of series part 3, "A rate limiter on a clock you control". It also solves the main ideas of part 2, "The busiest stretch" and part 4, "Give every message a send time". Solve those first. You may replace the given code, but keep the public names.


**Capstone, step 4 of 7.** Each client needs its own limiter. Clients come and go by the million, so old ones must be forgotten. But forgetting must never give anyone an allowance they would not have had.

The earlier rate limiter exercise forgot a client after a fixed idle time. If that time is too short, a client gets a fresh start too early. This time the rule is: **nobody can tell** that anything was forgotten.

## What to write

**`Forget::is_fresh(now_ms)`** for the four given limiters. It is true exactly when, at every time from `now_ms` on, the limiter answers every call as a new limiter with the same settings would. It changes nothing. An earlier time than the limiter has seen counts as that latest time.

**`Keyed`**, one limiter per key:

- `check(key, now_ms)` asks the key's limiter, making one with `make()` if none is held. A time earlier than one already given to `check` or `sweep` counts as that latest time, **for every key**.
- Its answers are the same as if no limiter were ever dropped.
- `sweep(now_ms)` drops **every** limiter that is fresh now, and returns how many.
- `len()` is the number held. Memory must follow the keys that matter, even if `sweep` is never called. After any `check`, `len() <= 2 × L + 64`. Here `L` is the largest number of limiters that were not fresh at the same moment so far.

A dropped limiter is dropped for real, and `Keyed` keeps no limiters besides the `len()` it holds. There may be hundreds of thousands of keys, so a call must not take time proportional to their number, on average.
