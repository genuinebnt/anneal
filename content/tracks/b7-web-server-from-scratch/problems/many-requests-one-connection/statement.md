Opening a connection costs time, so a client that has several requests sends them over the same one. It may even send the next request before the previous reply has arrived. Your job is the loop that serves one connection.

The starter gives you `next_request(buf)`, which reads one request from the front of the bytes you hold, and `reply(status, body, keep_open)`, which builds a reply. Use them as they are.

`serve(stream, max_requests, handler)` serves one connection until it is over, and returns how many requests were passed to `handler`. `max_requests` is at least 1.

- Each request gets `reply(200, handler(&request), keep_open)`, in the order the requests arrive.
- **The connection stays open** after a request unless one of these holds, in which case that reply says so and `serve` returns: the request's `Connection` header lists `close`; the request is HTTP/1.0 and its `Connection` header does not list `keep-alive`; or it is request number `max_requests`. The `Connection` header is a comma-separated list, compared without regard to case.
- When `next_request` reports that the client is waiting for permission to send its body (`Next::More { wants_continue: true }`), write `HTTP/1.1 100 Continue\r\n\r\n`, once for that request.
- On `Next::Invalid`, write `reply(400, "bad request", false)` and return.
- When the client ends its side, return. Bytes that never became a request are ignored.
- A read error of kind `Interrupted` is retried. Any other error from the stream is returned.

**A client may wait for your reply before it sends anything more**, so never read while you hold something you could answer. Flush after writing a reply or the permission.
