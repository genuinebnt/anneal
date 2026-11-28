Sometimes a sender does not know how long a body will be when it starts. It sends the body in chunks, each announcing its own size. Your server receives these bytes in arbitrary pieces, from a sender it does not trust.

The encoded form:

```
5\r\n
hello\r\n
6;note=ignored\r\n
 world\r\n
0\r\n
Optional-Trailer: x\r\n
\r\n
```

Each chunk is a size line (1 to 8 hexadecimal digits, optionally followed by `;` and any text, which is ignored), then exactly that many bytes of any value, then CRLF. A size of zero ends the chunks; then come zero or more trailer lines (their content is ignored) and an empty line.

`decode(buf, max_body)` decodes from the front of `buf`:

- `Ok(Some((body, used)))`: the encoding is complete; it took `used` bytes, and `body` is the chunks joined;
- `Ok(None)`: more bytes are needed;
- `Err(ChunkError::Malformed)`: the bytes break the form above;
- `Err(ChunkError::TooLarge)`: the body would exceed `max_body` bytes;
- `Err(ChunkError::TrailerTooLong)`: the trailer section, including the final empty line, is longer than `MAX_TRAILER` bytes.

A size line may be at most `MAX_LINE` bytes without its CRLF; longer is `Malformed`.

The sender must not be able to make you hold an unbounded amount: a refusal comes **before** the data of a chunk that is too large arrives, and without waiting for the end of an over-long line or trailer section. (Other refusals may wait for the end of the line they are in.) For every input: once a prefix is refused, every longer input gives the same error. Every strict prefix of a valid encoding gives `Ok(None)`.
