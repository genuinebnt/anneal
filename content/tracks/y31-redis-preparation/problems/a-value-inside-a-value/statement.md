Programs often send **nested** data over a connection: a list of strings and numbers, with lists inside. The bytes arrive in pieces, from a sender you do not trust.

The format has three kinds of value:

| Bytes | Value |
|---|---|
| `i-42e` | an integer: `i`, an optional `-`, digits, `e` |
| `5:hello` | bytes: a length, `:`, then that many bytes of any kind |
| `l` items `e` | a list of values |

Digits are ASCII. A number has no leading zeros (`0` itself is fine), and there is no `-0`. An integer must fit in an `i64`. A length must fit in a `u64`.

**`decode(buf, limits)`** looks at the value at the front of `buf`.

- `Ok(Some((value, used)))`: a whole value takes the first `used` bytes. Later bytes are left alone.
- `Ok(None)`: more bytes could still complete a valid value.
- `Err(..)`: **no bytes added later could make it valid** within the limits. Report this as soon as it becomes true, at the exact byte that makes it so.

| Error | When |
|---|---|
| `TooLong` | a byte string's length is over `max_bytes`, as soon as its digits say so |
| `TooDeep` | an `l` would make more than `max_depth` lists open at once |
| `TooMany` | a list that holds `max_items` items sees the first byte of another (`i`, `l` or a digit) |
| `Malformed` | anything else that cannot be valid |

A length that goes over `max_bytes` is `TooLong` even before its `:`. A length too large for `u64` is `Malformed`, which can happen only when `max_bytes` is that large.

The input may declare enormous sizes, with limits as high as `usize::MAX`. Do not allocate for what is only declared, and do not overflow. Deeply nested input must not crash the program.
