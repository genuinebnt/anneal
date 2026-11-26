A client sends small text requests to your service over a connection. The bytes arrive in whatever pieces the network chooses: half a request, exactly one, or three and the start of a fourth.

A request looks like this (every line ends in CRLF, written `\r\n`):

```
VERB name VERSION
Header-Name: value
Content-Length: 5

hello
```

The first line is three non-empty parts separated by single spaces; the third must be `MSG/1`. Then zero or more header lines `name:value`, an empty line, and a body of exactly `Content-Length` bytes (none without that header). Names are not empty; they are stored in lower case. Spaces and tabs around a value are removed. `Content-Length` (any case) is ASCII digits only; if it appears twice, or does not fit in a `usize`, the request is invalid. The lines before the body must be UTF-8. The body may be any bytes.

A real server must also refuse any name that is not an HTTP token (letters, digits and a few punctuation marks); these tests do not require that, but the reference code does it.

**`read_one(buf)`** looks at all the bytes received and not yet used:

- `Read::NeedMore`: they could still become a request;
- `Read::Whole(message, used)`: a whole request is at the front and takes `used` bytes;
- `Read::Invalid`: the head is complete and breaks a rule.

It changes nothing, so it can be called again when more bytes arrive.

**`take_all(buf)`** removes from the front of `buf` every whole request, and returns them in order. Bytes of an unfinished request stay. If the request at the front is invalid, it returns `Err(Invalid)` with the earlier requests in `Invalid::before`, leaving the bad bytes in `buf`.

However a stream of valid requests is cut into pieces, adding the pieces one at a time and calling `take_all` after each must give the same requests.
