Two clerks share one counter. Each reads it, adds one, and writes it back. Nearly every day the count is right. About once a month it is one short, and nobody can make it happen on purpose.

So stop using real threads. Give each clerk a `step` method that does one small thing. Let **your** loop decide who moves next, and take all its luck from one seed.

**`Dice`** is the generator xorshift64:

- `new(seed)` uses `seed` as the state. A seed of 0 uses `0x9E3779B97F4A7C15` instead.
- `next_u64()` does, in order: `x ^= x << 13`, `x ^= x >> 7`, `x ^= x << 17`. It stores `x` and returns it.
- `below(n)` is `next_u64() % n`, for `n > 0`.

A **worker** has `step(world)`, which returns `true` when the worker has finished.

**`run(world, workers, seed, max_steps)`** repeats this while a worker is unfinished and fewer than `max_steps` steps have been taken:

1. List the unfinished workers' indexes, in rising order.
2. Roll `below(len)` **once** and take that entry of the list.
3. Step that worker, and add its index to the trace.

It returns the trace, and whether every worker finished. Every worker starts unfinished.

**`replay(world, workers, trace)`** steps the workers in the order of the trace, without dice. It returns `Err(at)`, the position in the trace, at the first entry that names no worker or a finished one. The steps before it have been taken.

**`hunt(seeds, max_steps, make, holds)`** looks for a broken rule. For each seed in order, it takes a fresh world and workers from `make()` and runs them as `run` does. After **every step** it asks `holds(&world)`. At the first `false` it stops and returns the seed and the trace so far.

It returns `None` if no seed breaks the rule. A world that is wrong before any step is not asked about.
