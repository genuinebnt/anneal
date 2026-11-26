> **Spoiler before you open the editor.** The starter of this step contains reference code for steps 1 to 2. It is a stand-in, because the app cannot yet carry your own code from one step to the next. It solves those steps and much of the preparation parts before them, so solve those first. You may replace it with your own code; keep the public names.

**Capstone, step 3 of 10.** Turn a `Response` into bytes. Write the `Response` methods, `write_response` and `error_response`. `reason` is given.

- The builder methods return the response; `header` appends (repeats are kept). `text` sets `Content-Type: text/plain; charset=utf-8`. `header_value` finds the first header in any case.
- `write_response(out, response, head_only, keep_alive)` writes `HTTP/1.1 <status> <reason>`, the response's headers in order, `Connection: keep-alive` or `Connection: close`, the framing header, an empty line and the body, with CRLF line ends. It returns the number of bytes written. `out` may accept only part of a buffer per call. The first write error is returned.
- `Bytes` and `Empty` bodies are framed with `Content-Length`.
- A `Stream` body is sent with `Transfer-Encoding: chunked`. Each piece must be written and flushed before the next piece is asked from the iterator. When writing fails, stop asking. Empty pieces send nothing.
- `head_only` (a HEAD request): the same head, including the framing header, and no body bytes.
- Statuses 1xx, 204 and 304 have no body and no framing header.
- `error_response(e)` is a text response with the status of `e`.
