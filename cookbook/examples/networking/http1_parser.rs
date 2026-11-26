//! RECIPE 188 (protocols): HTTP/1.1 message parser and serializer: framing, request smuggling defence, pipelining, fuzzing.
//! LEVEL: advanced
//! Use in: HTTP servers/proxies, reverse proxies and gateways, test doubles, any text-headers-plus-body protocol.   Run: cargo run --release --example http1_parser
#![allow(dead_code)]
//!
//! WHAT: an HTTP/1.1 message is `request-line CRLF *(header CRLF) CRLF body`. The body length is NOT guessed: it comes from
//! exactly one of `Content-Length: n` (n raw bytes), `Transfer-Encoding: chunked` (hex-size lines, data, a zero chunk, then
//! optional trailer headers), or "no body" for requests with neither. See RFC 9112 section 6 (message body) and 7.1 (chunked).
//! WHY INCREMENTAL: TCP is a byte stream. `parse_request(&[u8])` returns `Ok(None)` ("need more bytes"),
//! `Ok(Some((request, consumed)))` or `Err` (the connection is poisoned: answer 400 and close). Re-parsing from the start is
//! simple and obviously correct, and the tests prove feeding one byte at a time equals parsing everything at once.
//! (Production code keeps resumable state or, like `httparse`, parses the head in one zero-copy pass.)
//! PIPELINING / KEEP-ALIVE: several requests may sit back to back in one buffer. Parse, drain `consumed`, repeat. HTTP/1.1
//! defaults to keep-alive unless `Connection: close`; HTTP/1.0 is the reverse.
//! REQUEST SMUGGLING: a front-end proxy and a back-end server that disagree about where a request ends can be tricked into
//! treating the tail of one request as the start of the next (cache poisoning, auth bypass). The classic triggers are both
//! `Content-Length` and `Transfer-Encoding` present (CL.TE / TE.CL), two different `Content-Length` values, obsolete line
//! folding, and whitespace before the colon. RFC 9112 says to reject or normalize; we REJECT every one of them with a typed error.
//! HOSTILE INPUT: every size is untrusted. `Limits` bounds line length, head bytes, header count, body bytes and the chunk
//! size field, and nothing is allocated from a declared length before the bytes have actually arrived.
//! COMMON USES: HTTP servers (hyper uses httparse), reverse proxies and load balancers, API gateways, WAF-style request
//! validation, CDN edge parsers, and test servers. Production Rust code should use `httparse` for the head and a
//! hardened framework for the rest; this recipe shows what they do.
//! Read: https://datatracker.ietf.org/doc/html/rfc9112 and https://github.com/seanmonstar/httparse (zero-copy production parser)

// ---------------------------------------------------------------- types

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HttpError {
    BareLf,
    LineTooLong,
    HeadTooLarge,
    TooManyHeaders,
    BadRequestLine,
    BadMethod,
    BadTarget,
    BadVersion,
    BadHeader,
    ObsFold,
    BadContentLength,
    ConflictingContentLength,
    Smuggling,
    UnsupportedTransferEncoding,
    BodyTooLarge,
    BadChunk,
    DuplicateHost,
    MissingHost,
    ForbiddenTrailer,
}

#[derive(Debug, Clone)]
struct Limits {
    max_line: usize,
    max_head: usize,
    max_headers: usize,
    max_body: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_line: 8 * 1024,
            max_head: 32 * 1024,
            max_headers: 100,
            max_body: 1 << 20,
        }
    }
}

type Headers = Vec<(String, String)>;

#[derive(Debug, Clone, PartialEq)]
struct Request {
    method: String,
    target: String,
    version: (u8, u8),
    headers: Headers,
    body: Vec<u8>,
    trailers: Headers,
}

fn lookup<'a>(h: &'a Headers, name: &str) -> Option<&'a str> {
    h.iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

impl Request {
    /// First value of a header, name compared case-insensitively.
    fn header(&self, name: &str) -> Option<&str> {
        lookup(&self.headers, name)
    }
    /// All values of a repeated header, in order (RFC 9110 5.3: equivalent to one comma-joined value).
    fn header_all(&self, name: &str) -> Vec<&str> {
        self.headers
            .iter()
            .filter(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
            .collect()
    }
    fn keep_alive(&self) -> bool {
        let tokens: Vec<String> = self
            .header_all("connection")
            .iter()
            .flat_map(|v| v.split(','))
            .map(|t| t.trim().to_ascii_lowercase())
            .collect();
        let has = |t: &str| tokens.iter().any(|x| x == t);
        if self.version == (1, 0) {
            has("keep-alive")
        } else {
            !has("close")
        }
    }
}

// ---------------------------------------------------------------- lexical helpers

fn is_token(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b)
}

/// One CRLF-terminated line. Bare LF is rejected (a classic desync: one side treats LF as a terminator, the other does not).
/// `Ok(None)` = line not complete yet.
fn read_line(buf: &[u8], pos: usize, max: usize) -> Result<Option<(&[u8], usize)>, HttpError> {
    match buf[pos..].iter().position(|&b| b == b'\n') {
        Some(rel) => {
            let i = pos + rel;
            if i == pos || buf[i - 1] != b'\r' {
                return Err(HttpError::BareLf);
            }
            let line = &buf[pos..i - 1];
            if line.len() > max {
                return Err(HttpError::LineTooLong);
            }
            if line.contains(&b'\r') {
                return Err(HttpError::BadHeader);
            }
            Ok(Some((line, i + 1)))
        }
        None if buf.len() - pos > max => Err(HttpError::LineTooLong),
        None => Ok(None),
    }
}

fn parse_header_line(line: &[u8]) -> Result<(String, String), HttpError> {
    if line[0] == b' ' || line[0] == b'\t' {
        return Err(HttpError::ObsFold); // obsolete line folding (RFC 9112 5.2): reject
    }
    let colon = line
        .iter()
        .position(|&b| b == b':')
        .ok_or(HttpError::BadHeader)?;
    let name = &line[..colon];
    // `Name : v` fails here because space is not a token char (a smuggling vector if one hop strips it).
    if name.is_empty() || !name.iter().all(|&b| is_token(b)) {
        return Err(HttpError::BadHeader);
    }
    let mut v = &line[colon + 1..];
    while let [b' ' | b'\t', rest @ ..] = v {
        v = rest;
    }
    while let [rest @ .., b' ' | b'\t'] = v {
        v = rest;
    }
    if v.iter().any(|&b| (b < 0x20 && b != b'\t') || b == 0x7f) {
        return Err(HttpError::BadHeader);
    }
    Ok((
        String::from_utf8_lossy(name).into_owned(),
        String::from_utf8_lossy(v).into_owned(),
    ))
}

// ---------------------------------------------------------------- head

fn parse_head(buf: &[u8], lim: &Limits) -> Result<Option<(Request, usize)>, HttpError> {
    let head_err = |buf: &[u8]| {
        if buf.len() > lim.max_head {
            Err(HttpError::HeadTooLarge)
        } else {
            Ok(None)
        }
    };
    let Some((line, mut pos)) = read_line(buf, 0, lim.max_line)? else {
        return head_err(buf);
    };
    let parts: Vec<&[u8]> = line.split(|&b| b == b' ').collect();
    if parts.len() != 3 {
        return Err(HttpError::BadRequestLine);
    }
    let (m, t, v) = (parts[0], parts[1], parts[2]);
    if m.is_empty() || m.len() > 32 || !m.iter().all(|&b| is_token(b)) {
        return Err(HttpError::BadMethod);
    }
    if t.is_empty() || !t.iter().all(|&b| (0x21..=0x7e).contains(&b)) {
        return Err(HttpError::BadTarget);
    }
    let version = match v {
        b"HTTP/1.1" => (1, 1),
        b"HTTP/1.0" => (1, 0),
        _ => return Err(HttpError::BadVersion),
    };
    let mut headers = Vec::new();
    loop {
        if pos > lim.max_head {
            return Err(HttpError::HeadTooLarge);
        }
        let Some((line, next)) = read_line(buf, pos, lim.max_line)? else {
            return head_err(buf);
        };
        pos = next;
        if line.is_empty() {
            break;
        }
        if headers.len() >= lim.max_headers {
            return Err(HttpError::TooManyHeaders);
        }
        headers.push(parse_header_line(line)?);
    }
    if pos > lim.max_head {
        return Err(HttpError::HeadTooLarge);
    }
    let req = Request {
        method: String::from_utf8_lossy(m).into_owned(),
        target: String::from_utf8_lossy(t).into_owned(),
        version,
        headers,
        body: Vec::new(),
        trailers: Vec::new(),
    };
    let hosts = req.header_all("host").len();
    if hosts > 1 {
        return Err(HttpError::DuplicateHost);
    }
    if hosts == 0 && version == (1, 1) {
        return Err(HttpError::MissingHost);
    }
    Ok(Some((req, pos)))
}

// ---------------------------------------------------------------- framing

#[derive(Debug, PartialEq)]
enum Framing {
    None,
    Length(usize),
    Chunked,
}

fn framing(req: &Request, lim: &Limits) -> Result<Framing, HttpError> {
    let cls = req.header_all("content-length");
    let tes = req.header_all("transfer-encoding");
    if !cls.is_empty() && !tes.is_empty() {
        return Err(HttpError::Smuggling); // CL + TE: never "pick one"; reject
    }
    if !tes.is_empty() {
        let codings: Vec<String> = tes
            .iter()
            .flat_map(|v| v.split(','))
            .map(|t| t.trim().to_ascii_lowercase())
            .collect();
        if codings.len() != 1 || codings[0] != "chunked" {
            return Err(HttpError::UnsupportedTransferEncoding); // also kills "chunked, chunked" and "xchunked" tricks
        }
        if req.version == (1, 0) {
            return Err(HttpError::UnsupportedTransferEncoding);
        }
        return Ok(Framing::Chunked);
    }
    let mut seen: Option<u64> = None;
    for v in cls.iter().flat_map(|v| v.split(',')).map(str::trim) {
        // digits only: no sign, no "0x", no empty, no spaces inside
        if v.is_empty() || v.len() > 18 || !v.bytes().all(|b| b.is_ascii_digit()) {
            return Err(HttpError::BadContentLength);
        }
        let n: u64 = v.parse().map_err(|_| HttpError::BadContentLength)?;
        match seen {
            Some(p) if p != n => return Err(HttpError::ConflictingContentLength),
            _ => seen = Some(n),
        }
    }
    match seen {
        None => Ok(Framing::None),
        Some(n) if n > lim.max_body as u64 => Err(HttpError::BodyTooLarge),
        Some(n) => Ok(Framing::Length(n as usize)),
    }
}

/// Decode a chunked body starting at `pos`: returns (body, trailers, position after the final CRLF).
/// Chunk extensions (`;name=value` after the size) are ignored.
fn parse_chunked(
    buf: &[u8],
    mut pos: usize,
    lim: &Limits,
) -> Result<Option<(Vec<u8>, Headers, usize)>, HttpError> {
    let mut body = Vec::new();
    loop {
        let Some((line, next)) = read_line(buf, pos, lim.max_line)? else {
            return Ok(None);
        };
        let size_part = line.split(|&b| b == b';').next().unwrap_or(&[]);
        if size_part.is_empty()
            || size_part.len() > 16
            || !size_part.iter().all(|b| b.is_ascii_hexdigit())
        {
            return Err(HttpError::BadChunk);
        }
        let size = u64::from_str_radix(std::str::from_utf8(size_part).unwrap(), 16)
            .map_err(|_| HttpError::BadChunk)?;
        pos = next;
        if size == 0 {
            let mut trailers = Vec::new();
            loop {
                let Some((line, next)) = read_line(buf, pos, lim.max_line)? else {
                    return Ok(None);
                };
                pos = next;
                if line.is_empty() {
                    return Ok(Some((body, trailers, pos)));
                }
                if trailers.len() >= lim.max_headers {
                    return Err(HttpError::TooManyHeaders);
                }
                let (k, v) = parse_header_line(line)?;
                // Trailers must not carry framing or routing fields (RFC 9110 6.5.1).
                if ["content-length", "transfer-encoding", "host", "trailer"]
                    .iter()
                    .any(|f| k.eq_ignore_ascii_case(f))
                {
                    return Err(HttpError::ForbiddenTrailer);
                }
                trailers.push((k, v));
            }
        }
        if size > (lim.max_body - body.len().min(lim.max_body)) as u64 {
            return Err(HttpError::BodyTooLarge);
        }
        let size = size as usize;
        if buf.len() < pos + size + 2 {
            return Ok(None);
        }
        body.extend_from_slice(&buf[pos..pos + size]);
        if &buf[pos + size..pos + size + 2] != b"\r\n" {
            return Err(HttpError::BadChunk);
        }
        pos += size + 2;
    }
}

/// Parse one complete request (head + body) from the front of `buf`.
fn parse_request(buf: &[u8], lim: &Limits) -> Result<Option<(Request, usize)>, HttpError> {
    let Some((mut req, pos)) = parse_head(buf, lim)? else {
        return Ok(None);
    };
    match framing(&req, lim)? {
        Framing::None => Ok(Some((req, pos))),
        Framing::Length(n) => {
            if buf.len() < pos + n {
                return Ok(None);
            }
            req.body = buf[pos..pos + n].to_vec();
            Ok(Some((req, pos + n)))
        }
        Framing::Chunked => match parse_chunked(buf, pos, lim)? {
            None => Ok(None),
            Some((body, trailers, end)) => {
                req.body = body;
                req.trailers = trailers;
                Ok(Some((req, end)))
            }
        },
    }
}

/// Pipelining: every complete request in `buf`, bytes consumed, and the error that stopped us (if any).
fn parse_all(buf: &[u8], lim: &Limits) -> (Vec<Request>, usize, Option<HttpError>) {
    let (mut out, mut at) = (Vec::new(), 0);
    loop {
        match parse_request(&buf[at..], lim) {
            Ok(Some((r, n))) => {
                debug_assert!(n > 0);
                out.push(r);
                at += n;
            }
            Ok(None) => return (out, at, None),
            Err(e) => return (out, at, Some(e)),
        }
    }
}

/// A connection-side buffer: feed bytes as they arrive, get completed requests back.
struct Conn {
    buf: Vec<u8>,
    lim: Limits,
    error: Option<HttpError>,
}

impl Conn {
    fn new(lim: Limits) -> Self {
        Conn {
            buf: Vec::new(),
            lim,
            error: None,
        }
    }
    fn feed(&mut self, bytes: &[u8]) -> Vec<Request> {
        let mut out = Vec::new();
        if self.error.is_some() {
            return out;
        }
        self.buf.extend_from_slice(bytes);
        loop {
            match parse_request(&self.buf, &self.lim) {
                Ok(Some((r, n))) => {
                    self.buf.drain(..n);
                    out.push(r);
                }
                Ok(None) => return out,
                Err(e) => {
                    self.error = Some(e);
                    return out;
                }
            }
        }
    }
}

// ---------------------------------------------------------------- response writer

struct Response {
    status: u16,
    reason: &'static str,
    headers: Headers,
    body: Vec<u8>,
}

impl Response {
    fn new(status: u16, reason: &'static str, body: &[u8]) -> Self {
        Response {
            status,
            reason,
            headers: vec![],
            body: body.to_vec(),
        }
    }
    fn header(mut self, k: &str, v: &str) -> Self {
        // Reject CR/LF in values: header injection / response splitting.
        assert!(!k.contains(['\r', '\n', ':']) && !v.contains(['\r', '\n']));
        self.headers.push((k.into(), v.into()));
        self
    }
    /// `chunk` = Some(n) streams the body as n-byte chunks with `Transfer-Encoding: chunked`.
    fn to_bytes(&self, keep_alive: bool, chunk: Option<usize>) -> Vec<u8> {
        let mut out = format!("HTTP/1.1 {} {}\r\n", self.status, self.reason).into_bytes();
        for (k, v) in &self.headers {
            out.extend_from_slice(format!("{k}: {v}\r\n").as_bytes());
        }
        out.extend_from_slice(if keep_alive {
            b"Connection: keep-alive\r\n"
        } else {
            b"Connection: close\r\n"
        });
        match chunk {
            None => out.extend_from_slice(
                format!("Content-Length: {}\r\n\r\n", self.body.len()).as_bytes(),
            ),
            Some(n) => {
                out.extend_from_slice(b"Transfer-Encoding: chunked\r\n\r\n");
                for c in self.body.chunks(n.max(1)) {
                    out.extend_from_slice(format!("{:x}\r\n", c.len()).as_bytes());
                    out.extend_from_slice(c);
                    out.extend_from_slice(b"\r\n");
                }
                out.extend_from_slice(b"0\r\n\r\n");
            }
        }
        if chunk.is_none() {
            out.extend_from_slice(&self.body);
        }
        out
    }
}

// ---------------------------------------------------------------- fuzzing support

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

const SEEDS: &[&[u8]] = &[
    b"GET /index.html HTTP/1.1\r\nHost: example.com\r\nAccept: */*\r\n\r\n",
    b"POST /submit HTTP/1.1\r\nHost: a\r\nContent-Length: 5\r\n\r\nhello",
    b"POST /up HTTP/1.1\r\nHost: a\r\nTransfer-Encoding: chunked\r\n\r\n5;ext=1\r\nhello\r\n3\r\nabc\r\n0\r\nX-Sum: 7\r\n\r\n",
    b"GET / HTTP/1.0\r\nConnection: keep-alive\r\n\r\nGET /b HTTP/1.0\r\n\r\n",
];

fn mutate(rng: &mut Rng, base: &[u8]) -> Vec<u8> {
    let mut v = base.to_vec();
    for _ in 0..1 + rng.below(4) {
        if v.is_empty() {
            v.push(rng.next() as u8);
            continue;
        }
        let i = rng.below(v.len());
        match rng.below(6) {
            0 => v[i] = rng.next() as u8,
            1 => v[i] = *b"\r\n: ;,\t0fF".get(rng.below(10)).unwrap(),
            2 => {
                v.remove(i);
            }
            3 => v.insert(i, rng.next() as u8),
            4 => v.truncate(i),
            _ => {
                let end = (i + 1 + rng.below(8)).min(v.len());
                let dup = v[i..end].to_vec();
                v.splice(i..i, dup);
            }
        }
    }
    v
}

fn outcome(buf: &[u8], lim: &Limits) -> (Vec<Request>, Option<HttpError>) {
    let (reqs, _, err) = parse_all(buf, lim);
    (reqs, err)
}

fn bytewise(buf: &[u8], lim: &Limits) -> (Vec<Request>, Option<HttpError>) {
    let mut c = Conn::new(lim.clone());
    let mut out = Vec::new();
    for b in buf {
        out.extend(c.feed(&[*b]));
    }
    (out, c.error)
}

fn main() {
    let lim = Limits::default();

    // 1. A plain request, case-insensitive lookup, keep-alive default.
    let raw = b"GET /a?b=1 HTTP/1.1\r\nHost: example.com\r\nuser-AGENT: demo\r\nAccept: a\r\nAccept: b\r\n\r\n";
    let (req, n) = parse_request(raw, &lim).unwrap().unwrap();
    assert_eq!(n, raw.len());
    assert_eq!(
        (req.method.as_str(), req.target.as_str(), req.version),
        ("GET", "/a?b=1", (1, 1))
    );
    assert_eq!(req.header("User-Agent"), Some("demo"));
    assert_eq!(req.header_all("ACCEPT"), vec!["a", "b"]);
    assert!(req.keep_alive());
    println!(
        "parsed: {} {} headers={}",
        req.method,
        req.target,
        req.headers.len()
    );

    // 2. Content-Length body and "need more bytes".
    let post = b"POST /x HTTP/1.1\r\nHost: h\r\nContent-Length: 5\r\n\r\nhello";
    assert!(
        parse_request(&post[..post.len() - 1], &lim)
            .unwrap()
            .is_none()
    );
    assert_eq!(parse_request(post, &lim).unwrap().unwrap().0.body, b"hello");

    // 3. Chunked body with extensions and trailers.
    let ch = b"POST /c HTTP/1.1\r\nHost: h\r\nTransfer-Encoding: Chunked\r\n\r\n4;a=b\r\nWiki\r\n5\r\npedia\r\n0\r\nExpires: never\r\n\r\n";
    let (r, n) = parse_request(ch, &lim).unwrap().unwrap();
    assert_eq!((r.body.as_slice(), n), (b"Wikipedia".as_slice(), ch.len()));
    assert_eq!(lookup(&r.trailers, "expires"), Some("never"));

    // 4. SMUGGLING: both framings, conflicting lengths, folding, space before colon, duplicate Host.
    let smuggle = |h: &str| {
        parse_request(
            format!("POST / HTTP/1.1\r\nHost: h\r\n{h}\r\n\r\n0\r\n\r\n").as_bytes(),
            &lim,
        )
    };
    assert_eq!(
        smuggle("Content-Length: 4\r\nTransfer-Encoding: chunked").unwrap_err(),
        HttpError::Smuggling
    );
    assert_eq!(
        smuggle("Content-Length: 4\r\nContent-Length: 5").unwrap_err(),
        HttpError::ConflictingContentLength
    );
    assert_eq!(
        smuggle("Content-Length: 4, 5").unwrap_err(),
        HttpError::ConflictingContentLength
    );
    assert_eq!(
        smuggle("Content-Length: +4").unwrap_err(),
        HttpError::BadContentLength
    );
    assert_eq!(
        smuggle("Transfer-Encoding: chunked, chunked").unwrap_err(),
        HttpError::UnsupportedTransferEncoding
    );
    assert_eq!(
        smuggle("Transfer-Encoding : chunked").unwrap_err(),
        HttpError::BadHeader
    );
    assert_eq!(smuggle("X: a\r\n folded").unwrap_err(), HttpError::ObsFold);
    assert_eq!(
        smuggle("Host: other").unwrap_err(),
        HttpError::DuplicateHost
    );
    // Identical duplicate Content-Length values are the one tolerated case (RFC 9110 8.6).
    assert!(smuggle("Content-Length: 3\r\nContent-Length: 3").is_ok());
    assert!(parse_request(b"GET / HTTP/1.1\r\nHost: h\n\r\n", &lim).is_err()); // bare LF
    println!(
        "smuggling vectors rejected: CL+TE, CL!=CL, TE-chunked-chunked, `TE :`, folding, bare LF"
    );

    // 5. Limits.
    let tiny = Limits {
        max_line: 32,
        max_head: 64,
        max_headers: 2,
        max_body: 8,
    };
    let long = format!("GET /{} HTTP/1.1\r\nHost: h\r\n\r\n", "a".repeat(100));
    assert_eq!(
        parse_request(long.as_bytes(), &tiny).unwrap_err(),
        HttpError::LineTooLong
    );
    assert_eq!(
        parse_request(b"GET / HTTP/1.1\r\nHost: h\r\nA: 1\r\nB: 2\r\n\r\n", &tiny).unwrap_err(),
        HttpError::TooManyHeaders
    );
    assert_eq!(
        parse_request(
            b"POST / HTTP/1.1\r\nHost: h\r\nContent-Length: 99\r\n\r\n",
            &tiny
        )
        .unwrap_err(),
        HttpError::BodyTooLarge
    );
    assert_eq!(
        parse_request(
            b"POST / HTTP/1.1\r\nHost: h\r\nTransfer-Encoding: chunked\r\n\r\nffffffffff\r\n",
            &lim
        )
        .unwrap_err(),
        HttpError::BodyTooLarge
    );

    // 6. Pipelining + keep-alive, and byte-at-a-time equals all-at-once.
    let mut pipe = Vec::new();
    for s in SEEDS {
        pipe.extend_from_slice(s);
    }
    let (reqs, used, err) = parse_all(&pipe, &lim);
    assert_eq!((reqs.len(), used, err), (5, pipe.len(), None));
    assert!(reqs[0].keep_alive() && reqs[3].keep_alive() && !reqs[4].keep_alive());
    assert_eq!(bytewise(&pipe, &lim), (reqs, None));
    println!(
        "pipelined 5 requests in {} bytes; byte-at-a-time identical",
        pipe.len()
    );

    // 7. Response writer (content-length and chunked) round trip through our own chunk decoder.
    let resp =
        Response::new(200, "OK", b"hello, chunked world").header("Content-Type", "text/plain");
    let plain = resp.to_bytes(true, None);
    assert!(plain.starts_with(b"HTTP/1.1 200 OK\r\n") && plain.ends_with(b"hello, chunked world"));
    let wire = resp.to_bytes(false, Some(6));
    let body_at = wire.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
    let (body, _, end) = parse_chunked(&wire, body_at, &lim).unwrap().unwrap();
    assert_eq!(
        (body.as_slice(), end),
        (b"hello, chunked world".as_slice(), wire.len())
    );
    println!(
        "response: {} bytes plain, {} bytes chunked",
        plain.len(),
        wire.len()
    );

    // 8. Fuzz: 20k random/mutated inputs: no panic, terminates, byte-wise verdict equals all-at-once.
    let small = Limits {
        max_line: 128,
        max_head: 512,
        max_headers: 16,
        max_body: 256,
    };
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let (mut ok, mut bad) = (0u32, 0u32);
    for i in 0..20_000 {
        let input = if i % 5 == 0 {
            (0..rng.below(200)).map(|_| rng.next() as u8).collect()
        } else {
            {
                let k = rng.below(SEEDS.len());
                mutate(&mut rng, SEEDS[k])
            }
        };
        let all = outcome(&input, &small);
        if all.1.is_some() {
            bad += 1
        } else {
            ok += 1
        }
        let bw = bytewise(&input, &small);
        // Same requests and same accept/reject verdict (the error KIND may differ: a partial line is
        // "too long" before the whole line shows it also contained a bare LF).
        assert!(
            all.0 == bw.0 && all.1.is_some() == bw.1.is_some(),
            "input {i}: {:?}",
            String::from_utf8_lossy(&input)
        );
    }
    println!("fuzz: 20000 inputs ({ok} clean, {bad} rejected), never panicked or looped");
}
