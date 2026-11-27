**Capstone, step 1 of 10.** Over ten steps you build a small Redis: a server that real Redis clients could talk to. It keeps five kinds of value, expires keys, runs transactions and blocking pops, publishes messages, and survives a restart. This step is its wire protocol, called RESP2.

**Frames.** Every frame starts with a type byte and has a line that ends in CRLF (`\r\n`).

| Bytes | Frame |
|---|---|
| `+text\r\n` | `Simple` (UTF-8) |
| `-text\r\n` | `Error` (UTF-8) |
| `:n\r\n` | `Int`: an optional `-`, then digits, within `i64` |
| `$n\r\n` then `n` bytes then `\r\n` | `Bulk`: any bytes |
| `$-1\r\n` | `Null` |
| `*n\r\n` then `n` frames | `Array` |
| `*-1\r\n` | `NullArray` |

**`parse_frame(buf, limits)`** reads one frame from the front of `buf`. It returns the frame and the number of bytes it takes, or `Ok(None)` while `buf` could still become a valid frame. Later bytes are left alone.

**Errors come as early as the bytes allow.** The sender is a stranger, so never wait for input that cannot help.

- `BadType(byte)`: a frame starts with a byte not in the table. Report it at that byte.
- `LineTooLong`: a line, from its type byte up to its CRLF, is longer than `max_line`. Report it once that is certain, even with no CRLF yet. A final `\r` may be the start of the CRLF.
- When a line is complete: `BadUtf8`, `BadInteger` (not a number as described, `+5` included), `BadLength` (a negative length other than -1).
- `TooLarge`: a length over `max_bulk` or `max_array`, reported when its line is complete.
- `BadTerminator`: a byte after a bulk payload is not the expected `\r` or `\n`. Report it when that byte is there.
- `TooDeep`: arrays nest more than `max_depth` deep. An outer array is depth 1, and `*-1` does not count. Report it when the line of the array that goes too deep is complete.

A declared size must not make you allocate for it, even if the limits are `usize::MAX`. Nor may it overflow your arithmetic.

**`parse_command(buf, limits)`** reads what a client sends, and returns its arguments.

- If the first byte is `*`, it is an array of bulk strings. The errors above apply, except that `max_depth` plays no part. `*0` and `*-1` give no arguments.
- An element that is not a bulk string is `ExpectedBulk(its first byte)`, reported as soon as that byte is there. A `$-1` element is `ExpectedBulk(b'$')` once its line is complete.
- Otherwise it is an **inline** command, for typing by hand: the bytes up to the first `\n`, without one `\r` before it, split on ASCII whitespace. An empty line gives no arguments.
- An inline line longer than `max_line` (without `\r\n`) is `LineTooLong`, whether its newline has arrived or not.

**`encode(frame, out)`** appends a frame's bytes. In `Simple` and `Error`, write every `\r` and `\n` as a space, so that text cannot forge a second reply. **`encode_command(args)`** is the array of bulk strings a client sends.
