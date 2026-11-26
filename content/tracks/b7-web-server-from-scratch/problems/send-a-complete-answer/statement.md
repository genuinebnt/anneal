Your server has an answer ready and a connection to send it on. The client must be able to tell exactly where the answer ends, because the next one may follow on the same connection. Some answers are produced slowly, piece by piece, and the client should see each piece as soon as it exists.

`send(out, reply, head_only)` writes a `Reply`:

1. `HTTP/1.1 <status> <reason>`
2. the reply's headers, in order, as `Name: value`
3. one framing header (below), then an empty line, then the body.

Every line ends in CRLF.

- `Content::Full(bytes)`: framed with `Content-Length`.
- `Content::Pieces(iterator)`: the length is not known in advance. Frame with `Transfer-Encoding: chunked`: each piece is sent as its length in hexadecimal, CRLF, the bytes, CRLF; after the last, `0`, CRLF, CRLF. Each piece must be written **and flushed** before the next one is asked from the iterator.
- `head_only` (the client asked only about the answer): the same lines, including the framing header, and no body bytes.
- Statuses 100 to 199, 204 and 304 never have a body or a framing header.

`out` is any byte sink. It may accept only part of what you give it per call, and may fail: return the first error, and do not ask for more pieces after it.

A reply must not be able to break the framing. If the reason, a header name or a header value contains a CR or LF, or a header is named `Content-Length` or `Transfer-Encoding` (in any case), return an error of kind `InvalidInput` **without writing anything**.
