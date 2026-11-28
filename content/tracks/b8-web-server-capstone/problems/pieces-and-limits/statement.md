> **Spoiler before you open the editor.** The starter of this step contains reference code for step 1. It is a stand-in, because the app cannot yet carry your own code from one step to the next. It solves that step and much of the preparation parts before them, so solve those first. You may replace it with your own code; keep the public names.

**Capstone, step 2 of 10.** Bytes now arrive in pieces, and not every peer is friendly. Extend `parse_request`. Step 1 keeps working.

**Pieces.** Any strict prefix of an acceptable request is `Incomplete`. Once some bytes are `Bad`, every longer buffer starting with them is `Bad` with the same error. `expect_continue` is true only when the head is complete, it has `Expect: 100-continue` (any case) and the body is not all there.

**Limits** (`Limits`; reaching one is fine, passing it is not). The request line and head size are enforced even when the end has not arrived; the header count and a `Content-Length` may wait until the head is complete:
- request line, without CRLF: `max_request_line`, else `UriTooLong`;
- the whole head including the blank line: `max_header_bytes`, else `HeaderTooLarge`; also more than `max_headers` header lines;
- body: `max_body`, else `BodyTooLarge`, as soon as its size is announced (for a chunk, before its data arrives). A length too large to fit a number is also `BodyTooLarge`.

**Strictness** (all `BadRequest`): a header line starting with a space or tab; a `Content-Length` that is not only ASCII digits; two `Content-Length` headers that differ (equal ones are fine); more than one `Transfer-Encoding`, one that is not exactly `chunked` (any case), one on HTTP/1.0, or one together with `Content-Length`.

**Chunked bodies.** With `Transfer-Encoding: chunked` the body is a series of chunks: a line with the size in hexadecimal (1 to 8 digits, nothing before them), optionally followed by `;` and an extension to be ignored, then that many bytes, then CRLF. A size of 0 ends the series; after it come zero or more trailer lines (ignored) and an empty line. A size line longer than 64 bytes without its CRLF is `BadRequest`. The trailer section, including the empty line, may not exceed `max_header_bytes` (`HeaderTooLarge`). `body` is the chunks joined.
