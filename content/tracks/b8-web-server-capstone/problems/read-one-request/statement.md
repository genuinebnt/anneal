**Capstone, step 1 of 10.** Over ten steps you build an HTTP/1.1 server using only the standard library. This step reads one request from bytes already in memory.

Write `parse_request(buf, limits)`, `Request::header` and `HttpError::status` (400, 414, 431, 413 in the order the variants are listed). `limits` is used from the next step on.

- A request is a request line, header lines, an empty line, and a body. Every line ends in CRLF. Everything before the body (the head) must be UTF-8.
- The request line is `METHOD target VERSION` with exactly one space between the parts. The method is one or more upper-case ASCII letters, the target starts with `/`, the version is `HTTP/1.1` or `HTTP/1.0`.
- A header is `name:value`. The name is not empty and holds no spaces or control bytes (so no space before the colon). Spaces and tabs around the value are removed. Names are stored in lower case; all headers are kept in order. `header(name)` finds the first with that name in any case. A real server must also refuse any name that is not an HTTP token (letters, digits and a few punctuation marks); these tests do not require that, but the reference code does it.
- HTTP/1.1 requires a `Host` header.
- `path` is the target up to the first `?`; `query` is the rest, without the `?`.
- The body is `Content-Length` bytes long, or empty without that header.
- Return `Done(request, used)` for a whole request at the front of `buf`; `used` counts its bytes only. Return `Bad(HttpError::BadRequest)` for a complete head that breaks a rule, and `Incomplete { expect_continue: false }` while bytes are missing.
