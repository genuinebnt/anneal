Your server reads requests from strangers. Before it reads a body, it must decide from the bytes so far whether to go on, wait, or refuse. A server that waits for the end of an endless line runs out of memory; a server that guesses what a strange request meant can be made to disagree with the proxy in front of it.

`check_head(buf, limits)` looks at the bytes received so far:

- `Ok(None)`: not refused yet, and the head is not complete;
- `Ok(Some(head))`: the head is complete and acceptable;
- `Err(refusal)`: no further bytes can make it acceptable. `Refusal::status` gives 400, 414, 431 and 413 in the order the variants are listed.

The head is a request line, header lines and an empty line, each ending in CRLF. The request line is `METHOD target VERSION` with exactly one space between the parts: METHOD is one or more upper-case ASCII letters, the target starts with `/`, VERSION is `HTTP/1.1` or `HTTP/1.0`. A header is `name:value`; names are stored in lower case, and spaces and tabs around values are removed. The head must be UTF-8. Bytes after the empty line are the body and are not examined.

**Limits** (reaching one is allowed, passing it is not), enforced even when the end has not arrived:

- the request line without its CRLF: `max_line`, else `LineTooLong`;
- the whole head including the empty line: `max_head`, else `HeadTooLarge`;
- more than `max_headers` header lines: `HeadTooLarge`;
- an announced body above `max_body`: `BodyTooLarge`, even if the number is too big for a `usize`.

**Refused as `BadRequest`:** a header without `:`; an empty name, or a name holding a space or a control byte (so no space before the colon); a line starting with a space or tab; HTTP/1.1 without `Host`; a `Content-Length` that is not only ASCII digits; two `Content-Length`s with different values (equal ones are fine); any `Transfer-Encoding` header (this service does not accept one).

`head.body_len` is the announced length, 0 if none. `head.head_len` counts the bytes up to and including the empty line.

A real server must also refuse any name that is not an HTTP token (letters, digits and a few punctuation marks); these tests do not require that, but the reference code does it.

**For all inputs:** if a prefix of `buf` is refused, every longer buffer that starts with it is refused with the same value. Every strict prefix of an acceptable head gives `Ok(None)`.
