**Capstone, step 4 of 10.** Two things are missing from the keyspace: the fifth kind of value, and a way to find keys. The starter holds the reference code of steps 1 to 3. It also gives `Ranked`, a set kept in score order, and `glob_matches`, which matches a key pattern.

**Spoiler warning.** The starter gives away steps 1 to 3 of this capstone. It also gives away work from these problems. Solve them first if you have not.

- *A value inside a value, arriving in pieces*: step 1 shows how to parse nested input in pieces
- *Requests that arrive in pieces*: step 1 shows the same need-more outcomes
- *Throw out what has gone stale*: `Deadlines` solves it
- *Who is in 40th place?*: `Ranked` solves its core, with `f64` scores
- *Which names fit the pattern?*: `glob_matches` solves it

**Sorted sets.** A sorted set holds distinct members, each with a score. Its order is by score, then by the member's bytes. They obey all the rules of the other collections, and `TYPE` calls them `zset`.

A set may hold 100000 members, so no command may walk past members it does not return.

- A score is any `f64` text Rust can parse, `inf`, `+inf` and `-inf` included, but **not NaN**. `-0` is `0`. Anything else is `ERR value is not a valid float`.
- A score is written back as Rust's `{}` writes an `f64`: `3`, `12.5`, `inf`.

| Command | Reply |
|---|---|
| `ZADD key score member [score member ...]` | the number of **new** members; a lone score is `ERR syntax error` |
| `ZREM key member [member ...]` | how many it removed |
| `ZCARD key` | the number of members |
| `ZSCORE key member` | the score as a string, or nil |
| `ZRANK key member` | the place in the order, from 0, or nil |
| `ZRANGE key start stop [WITHSCORES]` | members by place, indexes as in `LRANGE` |
| `ZRANGEBYSCORE key min max [WITHSCORES]` | members with a score from `min` to `max` |
| `ZCOUNT key min max` | how many that range holds |
| `ZINCRBY key amount member` | the new score; a missing member starts at 0 |

- `ZADD` checks every score before it changes anything.
- With `WITHSCORES`, each member is followed by its score. Any other word there is `ERR syntax error`.
- In `min` and `max`, a leading `(` excludes that end. A bad one is `ERR min or max is not a float`. A range with `min` above `max` is empty.
- If `ZINCRBY` would give NaN: `ERR resulting score is not a number (NaN)`, and nothing changes.

**`KEYS pattern`** returns the keys that `glob_matches` accepts, in byte order.

**`SCAN cursor [MATCH pattern] [COUNT n]`** walks the keys a few at a time. It replies `Array([Bulk(next_cursor), Array(keys)])`. `COUNT` comes from a stranger: every count up to `9223372036854775807` is valid, however few keys there are. Do not reserve memory or loop by it.

- The caller starts with cursor `0`, passes back each cursor it gets, and is done when it gets `0`. Other cursors may be any text you like, without spaces. A cursor you could not have given is `ERR invalid cursor` (the tests try only `hello`).
- One call **looks at** at most `n` keys (10 if not given), so it returns at most `n`. `MATCH` filters what was looked at; a call may return nothing and still not be done. With no keys added, `k` keys take no more than `k / n`, rounded up, plus one calls.
- A key that exists from the first call to the last is returned **exactly once**, whatever is added or removed between calls. You keep no state for a scan.
- `n` below 1, a missing value or another option is `ERR syntax error`. An `n` that is not an integer is the integer error.

Neither command returns an expired key.
