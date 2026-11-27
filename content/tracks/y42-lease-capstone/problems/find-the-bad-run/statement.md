> **Spoiler before you open the editor.** The starter holds reference code for **capstone steps 1 to 4** (the lease service, the disk and its judge, the client, the world). Solve those steps first.
>
> It also holds code that solves these preparation parts. Solve them first:
>
> - **"Luck you can run again"**
> - **"What happens next?"**
>
> You may replace the code with your own; keep the public names.

**Capstone, step 5 of 5.** You can stage an accident by hand. Real bugs are the accidents nobody thought of. So now draw the delays at random, run thousands of worlds, and find the ones that break. Each world comes from one seed, so any failure can be run again.

## `SeededDelays`

`SeededDelays::new(seed, pause_percent, ttl_ms)` is a `Delays`. A **stall** is a wait from `ttl_ms / 2` to `3 * ttl_ms`.

| Kind | Value, each end included |
|---|---|
| `Start` | 0 to 10 |
| `Net` | 1 to 20 |
| `Work` | 5 to 30; with a chance of `pause_percent` in 100, a stall **is added** |
| `Pause` | with a chance of `pause_percent` in 100, a stall; otherwise 0 |
| `Backoff` | 20 to 60 |
| `Rest` | 10 to 40 |

Within a range, every value should be possible. Sums and products that do not fit stop at `u64::MAX`.

**The rule that matters.** The two settings are fixed. Then an answer depends on four things only: the seed, the client, the kind and a count. The count is **how many times this client was asked for this kind before**. Calls about other clients or kinds, in any number and order, change nothing. Different clients, kinds and seeds must get **unrelated** delays: knowing one sequence must tell nothing about another.

## Running seeds

- `run_seed(config, pause_percent, seed)` simulates `config` with `SeededDelays`, using `config.ttl_ms`.
- `run_pair(config, pause_percent, seed)` runs the same seed twice: against an unguarded disk (`open`) and a guarded one (`guarded`). It ignores `config.guarded`.
- A run **breaks a rule** when `judge` on its report is an error.
- `first_breach(config, pause_percent, seeds)` tries the seeds in order on **unguarded** disks. It returns the first seed that breaks a rule, with the breach, or `None`. It stops at the first: the range may be `0..u64::MAX`.
- `campaign(config, pause_percent, seeds)` runs every seed as a pair and fills in `Summary`. `rejected` counts the arrived writes that the guarded disks did not accept.
- If any run returns an error, `first_breach` and `campaign` return that error.

## `story`

A trace of a bad run is long, and most of it does not matter. `story(report)` cuts it down when `judge` reports `StaleWrite { newer, stale }`:

- It starts at the entry where the service granted `stale.token`.
- It ends at the entry where the disk got the stale write (that token and value).
- In between, **both included**, it keeps only the entries of two clients: the one that sent `stale` and the one that sent `newer`. Order is unchanged.

For any other verdict it returns `None`. `story` reads only the report it is given, and a report may be written by hand. Every report with a stale write in the tests has both entries in its trace.

## Size

The tests run a few thousand small worlds in a debug build.
