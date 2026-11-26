> **Spoiler before you open the editor.** The starter of this step contains reference code for steps 1 to 6. It is a stand-in, because the app cannot yet carry your own code from one step to the next. It solves those steps and much of the preparation parts before them, so solve those first. You may replace it with your own code; keep the public names.

**Capstone, step 7 of 10.** One connection now carries many requests. `Config` gains `idle_timeout` (default 5 s), `request_timeout` (default 2 s) and `max_requests_per_conn` (default 100).

- HTTP/1.1 connections stay open unless the request's `Connection` header lists `close`. HTTP/1.0 connections close unless it lists `keep-alive`. The header is a comma-separated list, in any case. The response's `Connection` header says what the server will do.
- Pipelined requests are answered in order. A request already received is answered without waiting for more bytes.
- The response to request number `max_requests_per_conn` on a connection says `close`, and the connection closes.
- When a request has `Expect: 100-continue` and its body is not all there, send `HTTP/1.1 100 Continue` and an empty line, once for that request.
- A bad request gets its error reply after the replies to the requests before it, and the connection closes.
- **Idle:** if no byte of a next request has arrived `idle_timeout` after the connection opened or after the last reply, close without sending anything.
- **Slow:** if a request is not complete `request_timeout` after its first byte arrived, reply `408` and close, **however the bytes are arriving**.
- Replies sent before closing must still reach the client (step 5).
