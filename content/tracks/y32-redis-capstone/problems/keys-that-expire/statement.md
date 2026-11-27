**Capstone, step 3 of 10.** A cache forgets. Any key may now have a deadline, after which it is gone. The starter holds the reference code of steps 1 and 2, and `Deadlines`, which stores deadlines and sweeps through them.

**Spoiler warning.** The starter gives away steps 1 to 2 of this capstone. It also gives away work from these problems. Solve them first if you have not.

- *A value inside a value, arriving in pieces*: step 1 shows how to parse nested input in pieces
- *Requests that arrive in pieces*: step 1 shows the same need-more outcomes
- *Throw out what has gone stale*: `Deadlines` solves it

Make deadlines work in `Engine`. All times are in milliseconds, and `now` is the current one. Time comes only from `now`.

**The rule.** A key with deadline `t` exists while `now < t`. From `now == t` on, **every command treats it as missing**, including those that only count keys, like `DBSIZE`. A key made again has no deadline.

| Command | Reply |
|---|---|
| `EXPIRE key seconds`, `PEXPIRE key ms` | sets the deadline that far from `now`; 1, or 0 for a missing key |
| `PEXPIREAT key t` | sets the deadline to `t`; 1 or 0 |
| `PTTL key` | milliseconds left; -1 with no deadline, -2 for a missing key |
| `TTL key` | the same in seconds, rounded to the nearest, halves upwards |
| `PERSIST key` | removes the deadline; 1, or 0 if there was none or no key |

- A deadline that is not in the future, negative ones included, removes the key at once and replies 1.
- If the deadline cannot be computed within `i64`, reply `ERR invalid expire time in 'expire' command` (or `'pexpire'`), and change nothing.

**`SET key value [options]`**, in any letter case:

- `NX` sets only if the key is missing, `XX` only if it exists. When the condition fails, reply nil and change nothing.
- `EX seconds` and `PX ms` set a deadline from `now`. `PXAT t` sets the deadline `t`. A `t` in the past is accepted, and the key is then already gone.
- `KEEPTTL` keeps the old deadline.
- A value of 0 or less, or a deadline that does not fit in `u64`, is `ERR invalid expire time in 'set' command`. A value that is not an integer is the usual integer error.
- `ERR syntax error`: an unknown option, a missing value, `NX` with `XX`, two of `EX`, `PX` and `PXAT`, or one of them with `KEEPTTL`.

**Which commands keep a deadline.** Commands that change a value in place keep it: `APPEND`, the counters, and the list, hash and set commands. `SET` without `KEEPTTL` and `MSET` clear it. `RENAME` moves the source's deadline with its value, and the target's old one is lost.

**`cycle(now, dice)`** frees keys that nobody asks for. It runs one `Deadlines::sweep` with at most 16 rounds, removes those keys, and returns how many it removed. `dice.below(n)` gives a random number in `0..n`.
