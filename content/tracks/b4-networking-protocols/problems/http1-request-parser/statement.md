Implement the request-parsing half of an HTTP/1.1 server, as an incremental parser: `feed` bytes as they arrive from the socket,
and `next_request` returns `Ok(Some(request))` for each complete request (several may be pipelined in the bytes), `Ok(None)` when
it needs more bytes (consuming nothing), or `Err` when the stream is invalid.

Parse `METHOD SP target SP HTTP/1.0|1.1` (method uppercase letters, exactly one space between parts), then header lines
`name: value` (name is a token with **no whitespace before the colon**; value is trimmed of spaces and tabs; no obsolete line folding), then
`CRLF`. Body framing: `Content-Length: <digits>` (digits only, no sign), or `Transfer-Encoding: chunked` (hex sizes, optional `;extension`,
trailers skipped, final `0` chunk). **Reject** a request that has both framing headers, or two `Content-Length`s with different values.

Enforce `Limits` early: the head (request line plus headers plus blank line) may not exceed `max_head`, even while still incomplete; at most
`max_headers` header lines; a body larger than `max_body` is `BodyTooLarge` as soon as its size is announced, not after it arrives.
