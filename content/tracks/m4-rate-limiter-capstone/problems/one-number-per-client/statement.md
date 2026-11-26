> **Spoiler before you open the editor.** The starter contains reference code for steps 1 and 2, with a working `SlidingLog` and `TokenBucket`. It is a stand-in until the app can carry your own code from step to step.
>
> It solves those steps and both limiters of series part 3, "A rate limiter on a clock you control", retry time included. It also solves the main idea of part 2, "The busiest stretch". Solve those first. You may replace the given code, but keep the public names.


**Capstone, step 3 of 7.** Your gateways keep each client's limit in a shared store. The store holds **one number** per client. You want exactly the token bucket's behaviour from it.

The given `TokenBucket` is the reference. It starts with `burst` tokens, earns one every `interval_ms`, and holds at most `burst`. `Meter::take(now_ms, n)` takes `n` tokens at once, or none, and says when to try again.

## What to write

Two more types that give **the same answer to every call**, including the same wait, for every `burst` (0 too) and every `interval_ms >= 1`.

**`LeakyMeter`** keeps a level that drains by 1 each millisecond and never below 0. A request of `n` units adds `n × interval_ms`, if the level then stays at most `burst × interval_ms`. Like the bucket, it counts an earlier time as the latest one seen.

**`Gcra`** may keep only **one number** that changes. `save()` returns it, and `restore(burst, interval_ms, saved)` continues exactly where that limiter was. The tests save and restore it in the middle of traffic.

For `Gcra` only, callers promise that times never go back. A time that does go back must still never earn anything again.

`Err(None)` means the call can never succeed. A wait that does not fit in a `u64` is `u64::MAX`. `take(now, 0)` always succeeds. `check` is `take` of one unit.
