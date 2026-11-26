//! CAPSTONE 13 (networking): a mini multithreaded HTTP/1.1 server from scratch on std::net (thread pool, keep-alive, pipelining, router, static files)
//! LEVEL: capstone
//! Use in: understanding what hyper/nginx/tomcat do under the hood; small embedded admin servers; load-shedding designs.   Run: cargo run --release --example mini_http_server_thread_pool
#![allow(dead_code)]
//!
//! WHAT: a complete little HTTP/1.1 server with no HTTP crate: an acceptor thread, a FIXED pool of worker threads fed
//! through a BOUNDED queue, an incremental request parser, keep-alive and pipelining, a router with path parameters and
//! middleware, static files (ETag, Last-Modified, Range, traversal protection), a streaming chunked response,
//! `Expect: 100-continue`, HEAD, graceful shutdown and Prometheus-style /metrics. A load-test client with its own
//! latency histogram compares thread-per-connection, the pool and a tokio server that reuses the same parser and routes.
//!
//!   clients --TCP-> [acceptor] --try_send-> [ bounded queue (cap N) ] --recv-> [worker 0..W]
//!                        |                          full?                              | serve_connection():
//!                        +-- 503 + Retry-After <----+                                   |  read -> parse -> route
//!                                                                                       |  -> middleware -> write
//!   request_id -> logging -> timing -> catch_panic -> router -> handler                 +- loop (keep-alive)
//!
//! WHY a bounded queue: an unbounded queue turns overload into unbounded latency and memory; refusing early with
//! `503 Retry-After` keeps latency for admitted work flat (load shedding) and tells clients to back off. WHY a fixed
//! pool: the number of threads (and so memory and context switches) no longer depends on the number of clients.
//! The classic price: a worker is pinned to a connection while it is open, so idle keep-alive clients can starve
//! queued ones. Real servers bound that with idle timeouts and max-requests-per-connection (both here), or use
//! an event loop (the tokio comparison below) so an idle connection costs a few hundred bytes instead of a thread.
//!
//! HOW (the details that bite):
//!  * a TCP read can contain half a request, one request, or five; the parser is a pure function over the buffer that
//!    answers Incomplete / Done(request, bytes_used) / Bad(error), so pipelining is just "parse again on the rest".
//!  * limits everywhere: request line 2 KiB (414), header block 8 KiB or 64 headers (431), body 64 KiB (413), chunk
//!    size line 40 bytes (400), trailer section 8 KiB (431). Syntax and ambiguity (both Content-Length and chunked,
//!    or Transfer-Encoding twice: request-smuggling vectors) are 400. Errors close the connection.
//!  * a limit must give the same answer however the bytes are cut: an unfinished line may end in the CR of its CRLF,
//!    so that CR is not counted. The checks parse every prefix of requests at and just over the limits.
//!  * the read side uses 50 ms timeout slices so idle keep-alive connections notice shutdown and the idle timeout
//!    without a separate timer thread. The request timeout (slow loris, 408) is checked on every turn of the loop:
//!    checked only after a silent read, a client that keeps dripping bytes would escape it and block shutdown.
//!  * a handler panic is caught by middleware (500) and, as a second line, around the whole connection, so one bad
//!    request never costs the pool a thread.
//!  * closing a socket that still holds unread input sends RST, which can destroy the error response in flight; so
//!    error paths do a lingering close: shutdown(Write), drain briefly, then drop. It is bounded in bytes AND time.
//!    A refused (503) socket is handed to one `rejector` thread that drains all of them without blocking, so the
//!    acceptor never waits. Any bounded linger has a limit: bytes sent after it can still draw a reset, and when
//!    more than 64 refused sockets are open the oldest is dropped early.
//!  * HTTP/1.0 has no chunked coding: a streamed response to such a client is ended by closing the connection (the
//!    client then cannot tell a cut-off body from a complete one). Range applies to `GET` only, not `HEAD`.
//!  * the latency histogram is approximate, but the Prometheus `le` buckets are inclusive and need exact counters.
//!  * graceful shutdown: set `stopping`, wake the blocked `accept`, drop the queue sender (workers drain what is queued
//!    then exit), in-flight requests finish and answer `Connection: close`, idle connections are closed, all joined.
//!
//! COMMON USES: the shape of Apache's worker MPM, Tomcat's connector (acceptor + executor + bounded accept queue),
//! the Rust Book's final project, nginx upstream queues, any admin/metrics endpoint inside a service; patterns:
//! bounded queues and load shedding (503 + Retry-After), backpressure, incremental parsers over byte streams,
//! middleware chains, conditional GET (ETag), range requests for resumable downloads, lingering close.
//! Read: https://datatracker.ietf.org/doc/html/rfc9112 https://datatracker.ietf.org/doc/html/rfc9110 https://doc.rust-lang.org/book/ch21-02-multithreaded.html https://prometheus.io/docs/instrumenting/exposition_formats/
use std::collections::VecDeque;
use std::fs;
use std::io::{self, ErrorKind, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, AtomicUsize, Ordering::SeqCst};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, Barrier, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

// ======================================================================== histogram

/// Log-linear histogram (8 sub-buckets per power of two, so <= 12.5% relative error), lock-free `record`.
/// Values are microseconds and may be any `u64`. Same idea as HdrHistogram, in a page of code.
/// The `le` bounds of the Prometheus report fall inside these approximate buckets, so each bound also has an exact counter.
struct Histogram {
    buckets: Vec<AtomicU64>,
    /// `exact[i]` counts samples in `(LE_US[i - 1], LE_US[i]]`; larger samples appear only in `count`.
    exact: [AtomicU64; LE_US.len()],
    count: AtomicU64,
    sum: AtomicU64,
    max: AtomicU64,
}

const NB: usize = 16 + 8 * 60; // 16 exact values, then 8 buckets for each of 2^4 ..= 2^63: all of u64

/// Upper bounds (inclusive, microseconds) of the buckets in the Prometheus report.
const LE_US: [u64; 9] = [100, 250, 500, 1000, 2500, 5000, 10_000, 50_000, 100_000];

impl Histogram {
    fn new() -> Histogram {
        Histogram {
            buckets: (0..NB).map(|_| AtomicU64::new(0)).collect(),
            exact: Default::default(),
            count: AtomicU64::new(0),
            sum: AtomicU64::new(0),
            max: AtomicU64::new(0),
        }
    }
    fn index(v: u64) -> usize {
        if v < 16 {
            return v as usize;
        }
        let e = 63 - v.leading_zeros() as usize; // floor(log2 v) >= 4
        let sub = ((v >> (e - 3)) & 7) as usize; // the 3 bits after the leading one
        16 + (e - 4) * 8 + sub
    }
    /// Largest value that maps to bucket `idx`.
    fn upper(idx: usize) -> u64 {
        if idx < 16 {
            return idx as u64;
        }
        let k = idx - 16;
        let (e, sub) = (k / 8 + 4, (k % 8) as u64);
        let width = 1u64 << (e - 3);
        (8 + sub) * width + (width - 1) // in this order the last bucket ends at u64::MAX without overflow
    }
    fn record(&self, v: u64) {
        self.count.fetch_add(1, SeqCst); // first: `cumulative` is then never ahead of a `count` read after it
        self.buckets[Self::index(v)].fetch_add(1, SeqCst);
        if let Some(i) = LE_US.iter().position(|&le| v <= le) {
            self.exact[i].fetch_add(1, SeqCst);
        }
        self.sum.fetch_add(v, SeqCst);
        self.max.fetch_max(v, SeqCst);
    }
    fn count(&self) -> u64 {
        self.count.load(SeqCst)
    }
    /// Smallest bucket upper bound such that at least `q` of the samples are <= it (clamped to the true max).
    fn quantile(&self, q: f64) -> u64 {
        let n = self.count();
        if n == 0 {
            return 0;
        }
        let target = ((q * n as f64).ceil() as u64).max(1);
        let mut cum = 0;
        for (i, b) in self.buckets.iter().enumerate() {
            cum += b.load(SeqCst);
            if cum >= target {
                return Self::upper(i).min(self.max.load(SeqCst));
            }
        }
        self.max.load(SeqCst)
    }
    /// Prometheus cumulative buckets: for each bound in `LE_US`, the number of samples `<=` it.
    fn cumulative(&self) -> [u64; LE_US.len()] {
        let mut total = 0;
        std::array::from_fn(|i| {
            total += self.exact[i].load(SeqCst);
            total
        })
    }
}

// ======================================================================== HTTP types and parser

#[derive(Debug, Clone, PartialEq)]
enum HttpError {
    BadRequest(&'static str),
    UriTooLong,
    HeaderTooLarge,
    BodyTooLarge,
}

impl HttpError {
    fn status(&self) -> u16 {
        match self {
            HttpError::BadRequest(_) => 400,
            HttpError::UriTooLong => 414,
            HttpError::HeaderTooLarge => 431,
            HttpError::BodyTooLarge => 413,
        }
    }
    fn message(&self) -> &'static str {
        match self {
            HttpError::BadRequest(m) => m,
            HttpError::UriTooLong => "request line too long",
            HttpError::HeaderTooLarge => "request headers too large",
            HttpError::BodyTooLarge => "request body too large",
        }
    }
}

#[derive(Debug, Clone)]
struct Limits {
    max_request_line: usize,
    max_header_bytes: usize,
    max_headers: usize,
    max_body: usize,
}

#[derive(Debug, Clone, Default)]
struct Request {
    method: String,
    path: String,
    query: String,
    minor: u8,
    headers: Vec<(String, String)>, // names lower-cased
    body: Vec<u8>,
}

impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
}

#[derive(Debug)]
enum Parsed {
    /// Need more bytes; `expect_continue` = the head is complete, asks for 100-continue and the body is not yet here.
    Incomplete {
        expect_continue: bool,
    },
    Done(Request, usize),
    Bad(HttpError),
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Length of the content of a line whose CRLF has not arrived: a trailing CR may be the first half of it.
/// Every line limit must measure this, or the verdict depends on where the network cut the bytes.
fn unfinished_line_len(b: &[u8]) -> usize {
    b.strip_suffix(b"\r").unwrap_or(b).len()
}

/// Pure incremental parser: re-run it whenever more bytes arrive. Never panics on any input.
fn parse_request(buf: &[u8], lim: &Limits) -> Parsed {
    use HttpError::*;
    let incomplete = Parsed::Incomplete {
        expect_continue: false,
    };
    let Some(line_end) = find(buf, b"\r\n") else {
        return if unfinished_line_len(buf) > lim.max_request_line {
            Parsed::Bad(UriTooLong)
        } else {
            incomplete
        };
    };
    if line_end > lim.max_request_line {
        return Parsed::Bad(UriTooLong);
    }
    let Some(head_end) = find(buf, b"\r\n\r\n") else {
        return if buf.len() > lim.max_header_bytes {
            Parsed::Bad(HeaderTooLarge)
        } else {
            incomplete
        };
    };
    let head_len = head_end + 4;
    if head_len > lim.max_header_bytes {
        return Parsed::Bad(HeaderTooLarge);
    }
    let Ok(head) = std::str::from_utf8(&buf[..head_end]) else {
        return Parsed::Bad(BadRequest("non-UTF-8 bytes in the header block"));
    };
    let mut lines = head.split("\r\n");
    let parts: Vec<&str> = lines.next().unwrap_or("").split(' ').collect();
    if parts.len() != 3 {
        return Parsed::Bad(BadRequest("malformed request line"));
    }
    let (method, target, version) = (parts[0], parts[1], parts[2]);
    if method.is_empty() || !method.bytes().all(|b| b.is_ascii_uppercase()) {
        return Parsed::Bad(BadRequest("bad method"));
    }
    let minor = match version {
        "HTTP/1.1" => 1,
        "HTTP/1.0" => 0,
        _ => return Parsed::Bad(BadRequest("unsupported HTTP version")),
    };
    if !target.starts_with('/') {
        return Parsed::Bad(BadRequest("target must be an absolute path"));
    }
    let mut headers: Vec<(String, String)> = Vec::new();
    for line in lines {
        if headers.len() >= lim.max_headers {
            return Parsed::Bad(HeaderTooLarge);
        }
        if line.starts_with(' ') || line.starts_with('\t') {
            return Parsed::Bad(BadRequest("obsolete line folding is rejected"));
        }
        let Some((name, value)) = line.split_once(':') else {
            return Parsed::Bad(BadRequest("header line without a colon"));
        };
        if name.is_empty() || name.bytes().any(|b| b <= b' ' || b == 0x7f) {
            return Parsed::Bad(BadRequest("bad header name"));
        }
        headers.push((name.to_ascii_lowercase(), value.trim().to_string()));
    }
    let get = |n: &str| {
        headers
            .iter()
            .find(|(k, _)| k == n)
            .map(|(_, v)| v.as_str())
    };
    if minor == 1 && get("host").is_none() {
        return Parsed::Bad(BadRequest("HTTP/1.1 requires a Host header"));
    }
    // Content-Length: all copies must agree and be plain digits.
    let mut cl: Option<usize> = None;
    for (k, v) in &headers {
        if k == "content-length" {
            if v.is_empty() || !v.bytes().all(|b| b.is_ascii_digit()) {
                return Parsed::Bad(BadRequest("bad Content-Length"));
            }
            let Ok(n) = v.parse::<usize>() else {
                return Parsed::Bad(BodyTooLarge);
            };
            if cl.is_some_and(|p| p != n) {
                return Parsed::Bad(BadRequest("conflicting Content-Length headers"));
            }
            cl = Some(n);
        }
    }
    // Reading only the first of two copies would let a proxy that reads the other one disagree about the framing.
    if headers
        .iter()
        .filter(|(k, _)| k == "transfer-encoding")
        .count()
        > 1
    {
        return Parsed::Bad(BadRequest("repeated Transfer-Encoding"));
    }
    let chunked = match get("transfer-encoding") {
        None => false,
        Some(v) if v.eq_ignore_ascii_case("chunked") => true,
        Some(_) => return Parsed::Bad(BadRequest("unsupported Transfer-Encoding")),
    };
    if chunked && cl.is_some() {
        return Parsed::Bad(BadRequest("both Content-Length and Transfer-Encoding"));
    }
    let expect = get("expect").is_some_and(|v| v.eq_ignore_ascii_case("100-continue"));
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let mut req = Request {
        method: method.to_string(),
        path: path.to_string(),
        query: query.to_string(),
        minor,
        headers: headers.clone(),
        body: Vec::new(),
    };
    if chunked {
        return match parse_chunked(buf, head_len, lim) {
            Err(e) => Parsed::Bad(e),
            Ok(None) => Parsed::Incomplete {
                expect_continue: expect,
            },
            Ok(Some((body, used))) => {
                req.body = body;
                Parsed::Done(req, used)
            }
        };
    }
    let n = cl.unwrap_or(0);
    if n > lim.max_body {
        return Parsed::Bad(BodyTooLarge); // answered before the body is read; no 100-continue is sent
    }
    if buf.len() < head_len + n {
        return Parsed::Incomplete {
            expect_continue: expect,
        };
    }
    req.body = buf[head_len..head_len + n].to_vec();
    Parsed::Done(req, head_len + n)
}

/// Longest chunk-size line (size and extensions, without its CRLF).
const MAX_CHUNK_LINE: usize = 40;

/// Decode a chunked body starting at `pos`. Ok(None) = need more bytes.
fn parse_chunked(
    buf: &[u8],
    mut pos: usize,
    lim: &Limits,
) -> Result<Option<(Vec<u8>, usize)>, HttpError> {
    let mut body = Vec::new();
    loop {
        let too_long = HttpError::BadRequest("chunk size line too long");
        let Some(e) = find(&buf[pos..], b"\r\n") else {
            return if unfinished_line_len(&buf[pos..]) > MAX_CHUNK_LINE {
                Err(too_long)
            } else {
                Ok(None)
            };
        };
        if e > MAX_CHUNK_LINE {
            return Err(too_long);
        }
        let line = std::str::from_utf8(&buf[pos..pos + e])
            .map_err(|_| HttpError::BadRequest("bad chunk size"))?;
        let hex = line.split(';').next().unwrap_or("").trim(); // ignore chunk extensions
        if hex.is_empty() || hex.len() > 8 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(HttpError::BadRequest("bad chunk size"));
        }
        let size = usize::from_str_radix(hex, 16).unwrap_or(usize::MAX);
        pos += e + 2;
        if size == 0 {
            // Trailer section: skip lines until the empty one. The body limit does not cover these bytes, so the
            // whole section (with the final CRLF) shares the size limit of the header block.
            let start = pos;
            loop {
                let Some(e) = find(&buf[pos..], b"\r\n") else {
                    // at least one more byte is still to come
                    return if buf.len() - start >= lim.max_header_bytes {
                        Err(HttpError::HeaderTooLarge)
                    } else {
                        Ok(None)
                    };
                };
                pos += e + 2;
                if pos - start > lim.max_header_bytes {
                    return Err(HttpError::HeaderTooLarge);
                }
                if e == 0 {
                    return Ok(Some((body, pos)));
                }
            }
        }
        if body.len() + size > lim.max_body {
            return Err(HttpError::BodyTooLarge);
        }
        if buf.len() < pos + size + 2 {
            return Ok(None);
        }
        if &buf[pos + size..pos + size + 2] != b"\r\n" {
            return Err(HttpError::BadRequest("chunk not terminated by CRLF"));
        }
        body.extend_from_slice(&buf[pos..pos + size]);
        pos += size + 2;
    }
}

fn wants_keep_alive(req: &Request) -> bool {
    let conn = req.header("connection").unwrap_or("").to_ascii_lowercase();
    let has = |t: &str| conn.split(',').any(|x| x.trim() == t);
    if has("close") {
        return false;
    }
    req.minor == 1 || has("keep-alive")
}

fn pct_decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = |c: &u8| char::from(*c).to_digit(16); // `from_str_radix` would also accept a sign: "+5"
            let [hi, lo] = b.get(i + 1..i + 3)? else {
                return None;
            };
            out.push((hex(hi)? * 16 + hex(lo)?) as u8);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

// ======================================================================== responses

enum Body {
    Empty,
    Bytes(Vec<u8>),
    Stream(Box<dyn Iterator<Item = Vec<u8>> + Send>),
}

struct Response {
    status: u16,
    headers: Vec<(String, String)>,
    body: Body,
}

impl Response {
    fn new(status: u16) -> Response {
        Response {
            status,
            headers: Vec::new(),
            body: Body::Empty,
        }
    }
    fn text(status: u16, s: impl Into<String>) -> Response {
        Response::new(status)
            .header("Content-Type", "text/plain; charset=utf-8")
            .bytes(s.into().into_bytes())
    }
    fn bytes(mut self, b: Vec<u8>) -> Response {
        self.body = Body::Bytes(b);
        self
    }
    fn header(mut self, k: &str, v: impl Into<String>) -> Response {
        self.headers.push((k.to_string(), v.into()));
        self
    }
}

fn reason(status: u16) -> &'static str {
    match status {
        100 => "Continue",
        200 => "OK",
        206 => "Partial Content",
        304 => "Not Modified",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        413 => "Content Too Large",
        414 => "URI Too Long",
        416 => "Range Not Satisfiable",
        431 => "Request Header Fields Too Large",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "Unknown",
    }
}

/// HTTP/1.0 has no chunked coding, so a streamed body sent to such a client has no length: closing ends it.
fn can_keep_alive(minor: u8, resp: &Response) -> bool {
    minor >= 1 || !matches!(resp.body, Body::Stream(_))
}

/// Serialise a response for a client speaking HTTP/1.`minor`. `head_only` (HEAD) sends the same headers, including Content-Length, but no body.
/// Works on any `Write`, so the tokio server reuses it with a `Vec<u8>`.
fn write_response<W: Write>(
    w: &mut W,
    resp: Response,
    head_only: bool,
    keep_alive: bool,
    minor: u8,
) -> io::Result<u64> {
    let keep_alive = keep_alive && can_keep_alive(minor, &resp);
    let mut out: Vec<u8> = Vec::with_capacity(256);
    write!(
        out,
        "HTTP/1.1 {} {}\r\nServer: mini-http/1.0\r\n",
        resp.status,
        reason(resp.status)
    )?;
    for (k, v) in &resp.headers {
        write!(out, "{k}: {v}\r\n")?;
    }
    write!(
        out,
        "Connection: {}\r\n",
        if keep_alive { "keep-alive" } else { "close" }
    )?;
    let bodyless = resp.status / 100 == 1 || resp.status == 204 || resp.status == 304;
    match resp.body {
        Body::Stream(chunks) => {
            let chunked = minor >= 1;
            if chunked {
                out.extend_from_slice(b"Transfer-Encoding: chunked\r\n");
            }
            out.extend_from_slice(b"\r\n");
            let mut total = out.len() as u64;
            w.write_all(&out)?;
            if !head_only {
                for c in chunks.filter(|c| !c.is_empty()) {
                    let f = if chunked {
                        let mut f = format!("{:x}\r\n", c.len()).into_bytes();
                        f.extend_from_slice(&c);
                        f.extend_from_slice(b"\r\n");
                        f
                    } else {
                        c
                    };
                    w.write_all(&f)?;
                    w.flush()?; // each chunk leaves immediately: that is what "streaming" means
                    total += f.len() as u64;
                }
                if chunked {
                    w.write_all(b"0\r\n\r\n")?;
                    total += 5;
                }
            }
            Ok(total)
        }
        other => {
            let bytes = if let Body::Bytes(b) = other {
                b
            } else {
                Vec::new()
            };
            if !bodyless {
                write!(out, "Content-Length: {}\r\n", bytes.len())?;
            }
            out.extend_from_slice(b"\r\n");
            if !head_only && !bodyless {
                out.extend_from_slice(&bytes);
            }
            w.write_all(&out)?;
            Ok(out.len() as u64)
        }
    }
}

fn error_response(e: &HttpError) -> Response {
    Response::text(
        e.status(),
        format!("{} {}: {}\n", e.status(), reason(e.status()), e.message()),
    )
}

// ======================================================================== router and middleware

struct Params(Vec<(String, String)>);

impl Params {
    fn get(&self, k: &str) -> &str {
        self.0
            .iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.as_str())
            .unwrap_or("")
    }
}

type Handler = Arc<dyn Fn(&Request, &Params) -> Response + Send + Sync>;
type Next<'a> = &'a dyn Fn(&Request) -> Response;
type Middleware = Arc<dyn Fn(&Request, Next) -> Response + Send + Sync>;

enum Seg {
    Lit(String),
    Param(String),
    Rest(String),
}

struct Route {
    method: &'static str,
    segs: Vec<Seg>,
    handler: Handler,
}

impl Route {
    fn matches(&self, path: &[&str]) -> Option<Params> {
        let mut out = Vec::new();
        for (i, s) in self.segs.iter().enumerate() {
            match s {
                Seg::Lit(l) => {
                    if path.get(i) != Some(&l.as_str()) {
                        return None;
                    }
                }
                Seg::Param(n) => match path.get(i) {
                    Some(v) if !v.is_empty() => out.push((n.clone(), v.to_string())),
                    _ => return None,
                },
                Seg::Rest(n) => {
                    out.push((n.clone(), path.get(i..)?.join("/")));
                    return Some(Params(out));
                }
            }
        }
        (path.len() == self.segs.len()).then_some(Params(out))
    }
}

#[derive(Default)]
struct Router {
    routes: Vec<Route>,
}

impl Router {
    /// Patterns: `/users/:id/posts/:pid`, `/static/*path` (the rest of the path).
    fn add(
        &mut self,
        method: &'static str,
        pattern: &str,
        f: impl Fn(&Request, &Params) -> Response + Send + Sync + 'static,
    ) {
        let segs = pattern
            .trim_start_matches('/')
            .split('/')
            .map(|s| match s.chars().next() {
                Some(':') => Seg::Param(s[1..].to_string()),
                Some('*') => Seg::Rest(s[1..].to_string()),
                _ => Seg::Lit(s.to_string()),
            })
            .collect();
        self.routes.push(Route {
            method,
            segs,
            handler: Arc::new(f),
        });
    }

    fn dispatch(&self, req: &Request) -> Response {
        let segs: Vec<&str> = req.path.trim_start_matches('/').split('/').collect();
        let mut allowed: Vec<&str> = Vec::new();
        for r in &self.routes {
            if let Some(p) = r.matches(&segs) {
                if r.method == req.method || (req.method == "HEAD" && r.method == "GET") {
                    return (r.handler)(req, &p);
                }
                allowed.push(r.method);
            }
        }
        if allowed.is_empty() {
            Response::text(404, format!("no route for {}\n", req.path))
        } else {
            Response::text(405, "method not allowed\n").header("Allow", allowed.join(", "))
        }
    }
}

fn run_chain(mws: &[Middleware], req: &Request, endpoint: Next) -> Response {
    match mws.split_first() {
        None => endpoint(req),
        Some((m, rest)) => m(req, &|r: &Request| run_chain(rest, r, endpoint)),
    }
}

struct App {
    router: Router,
    mws: Vec<Middleware>,
    metrics: Arc<Metrics>,
}

impl App {
    fn handle(&self, req: &Request) -> Response {
        run_chain(&self.mws, req, &|r: &Request| self.router.dispatch(r))
    }
}

// ======================================================================== metrics

struct Metrics {
    accepted: AtomicU64,
    rejected: AtomicU64,
    active: AtomicI64,
    queued: AtomicI64,
    requests: AtomicU64,
    class: [AtomicU64; 6], // index = status / 100
    handler_panics: AtomicU64,
    worker_panics: AtomicU64,
    reused: AtomicU64, // requests served on an already-used connection
    idle_closed: AtomicU64,
    bytes_out: AtomicU64,
    latency: Histogram,
}

impl Metrics {
    fn new() -> Metrics {
        Metrics {
            accepted: AtomicU64::new(0),
            rejected: AtomicU64::new(0),
            active: AtomicI64::new(0),
            queued: AtomicI64::new(0),
            requests: AtomicU64::new(0),
            class: Default::default(),
            handler_panics: AtomicU64::new(0),
            worker_panics: AtomicU64::new(0),
            reused: AtomicU64::new(0),
            idle_closed: AtomicU64::new(0),
            bytes_out: AtomicU64::new(0),
            latency: Histogram::new(),
        }
    }
    fn count_response(&self, status: u16) {
        self.requests.fetch_add(1, SeqCst);
        self.class[(status / 100).min(5) as usize].fetch_add(1, SeqCst);
    }
    /// Prometheus text exposition format (counters, gauges and a cumulative histogram).
    fn render(&self) -> String {
        let mut s = String::new();
        let mut metric = |name: &str, help: &str, kind: &str, v: i64| {
            s.push_str(&format!(
                "# HELP {name} {help}\n# TYPE {name} {kind}\n{name} {v}\n"
            ));
        };
        metric(
            "http_connections_accepted_total",
            "Connections accepted.",
            "counter",
            self.accepted.load(SeqCst) as i64,
        );
        metric(
            "http_connections_rejected_total",
            "Connections shed with 503 (queue full).",
            "counter",
            self.rejected.load(SeqCst) as i64,
        );
        metric(
            "http_connections_active",
            "Connections currently open.",
            "gauge",
            self.active.load(SeqCst),
        );
        metric(
            "http_queue_depth",
            "Connections waiting for a worker.",
            "gauge",
            self.queued.load(SeqCst),
        );
        metric(
            "http_requests_total",
            "Requests answered.",
            "counter",
            self.requests.load(SeqCst) as i64,
        );
        metric(
            "http_requests_reused_total",
            "Requests served on a reused keep-alive connection.",
            "counter",
            self.reused.load(SeqCst) as i64,
        );
        metric(
            "http_handler_panics_total",
            "Handler panics turned into 500.",
            "counter",
            self.handler_panics.load(SeqCst) as i64,
        );
        metric(
            "http_idle_closed_total",
            "Idle keep-alive connections closed.",
            "counter",
            self.idle_closed.load(SeqCst) as i64,
        );
        metric(
            "http_response_bytes_total",
            "Bytes written.",
            "counter",
            self.bytes_out.load(SeqCst) as i64,
        );
        s.push_str("# HELP http_responses_total Responses by status class.\n# TYPE http_responses_total counter\n");
        for c in 1..=5 {
            s.push_str(&format!(
                "http_responses_total{{class=\"{c}xx\"}} {}\n",
                self.class[c].load(SeqCst)
            ));
        }
        s.push_str("# HELP http_request_duration_us Handler latency in microseconds.\n# TYPE http_request_duration_us histogram\n");
        for (le, n) in LE_US.iter().zip(self.latency.cumulative()) {
            s.push_str(&format!(
                "http_request_duration_us_bucket{{le=\"{le}\"}} {n}\n"
            ));
        }
        s.push_str(&format!(
            "http_request_duration_us_bucket{{le=\"+Inf\"}} {}\n",
            self.latency.count()
        ));
        s.push_str(&format!(
            "http_request_duration_us_sum {}\nhttp_request_duration_us_count {}\n",
            self.latency.sum.load(SeqCst),
            self.latency.count()
        ));
        s
    }
}

/// Reads a metric line `name value` from Prometheus text (unlabelled series only).
fn metric_value(text: &str, name: &str) -> Option<i64> {
    text.lines()
        .find_map(|l| l.strip_prefix(name)?.strip_prefix(' ')?.trim().parse().ok())
}

// ======================================================================== static files

fn http_date(t: SystemTime) -> String {
    let secs = t
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64;
    let (days, rem) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    // civil-from-days (Howard Hinnant): days since 1970-01-01 -> y/m/d
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    const WD: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    const MN: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    format!(
        "{}, {:02} {} {} {:02}:{:02}:{:02} GMT",
        WD[((days + 4).rem_euclid(7)) as usize],
        d,
        MN[(m - 1) as usize],
        y,
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

enum RangeSpec {
    Ignore,
    Bytes(usize, usize), // inclusive
    Unsatisfiable,
}

fn parse_range(h: &str, len: usize) -> RangeSpec {
    let Some(spec) = h.trim().strip_prefix("bytes=") else {
        return RangeSpec::Ignore;
    };
    if spec.contains(',') {
        return RangeSpec::Ignore; // multipart/byteranges is optional; serving the full body is allowed
    }
    let Some((a, b)) = spec.split_once('-') else {
        return RangeSpec::Ignore;
    };
    if a.is_empty() {
        // suffix range: the last n bytes
        return match b.parse::<usize>() {
            Ok(0) => RangeSpec::Unsatisfiable,
            Ok(n) if len > 0 => RangeSpec::Bytes(len.saturating_sub(n), len - 1),
            Ok(_) => RangeSpec::Unsatisfiable,
            Err(_) => RangeSpec::Ignore,
        };
    }
    let Ok(start) = a.parse::<usize>() else {
        return RangeSpec::Ignore;
    };
    let end = if b.is_empty() {
        usize::MAX
    } else {
        match b.parse::<usize>() {
            Ok(e) if e >= start => e,
            _ => return RangeSpec::Ignore, // syntactically invalid: ignore the header
        }
    };
    if start >= len {
        return RangeSpec::Unsatisfiable;
    }
    RangeSpec::Bytes(start, end.min(len - 1))
}

fn content_type(p: &Path) -> &'static str {
    match p.extension().and_then(|e| e.to_str()) {
        Some("txt") => "text/plain; charset=utf-8",
        Some("html") => "text/html; charset=utf-8",
        Some("json") => "application/json",
        _ => "application/octet-stream",
    }
}

fn serve_static(root: &Path, rest: &str, req: &Request) -> Response {
    let forbidden = || Response::text(403, "403 Forbidden\n");
    let Some(decoded) = pct_decode(rest) else {
        return Response::text(400, "bad percent-encoding\n");
    };
    if decoded.contains('\0') || decoded.contains('\\') {
        return forbidden();
    }
    let mut p = root.to_path_buf();
    for seg in decoded.split('/') {
        match seg {
            "" | "." => {}
            ".." => return forbidden(), // layer 1: refuse any parent segment, even if it would stay inside
            s => p.push(s),
        }
    }
    let Ok(canon) = fs::canonicalize(&p) else {
        return Response::text(404, "not found\n");
    };
    if !canon.starts_with(root) {
        return forbidden(); // layer 2: symlinks that escape the root resolve outside it
    }
    let Ok(meta) = fs::metadata(&canon) else {
        return Response::text(404, "not found\n");
    };
    if !meta.is_file() {
        return Response::text(404, "not found\n");
    }
    let Ok(data) = fs::read(&canon) else {
        return Response::text(404, "not found\n");
    };
    let mtime = meta.modified().unwrap_or(UNIX_EPOCH);
    let etag = format!(
        "\"{:x}-{:x}\"",
        data.len(),
        mtime
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    let last_modified = http_date(mtime);
    let base = |r: Response| {
        r.header("ETag", etag.clone())
            .header("Last-Modified", last_modified.clone())
            .header("Accept-Ranges", "bytes")
    };
    // Conditional GET: If-None-Match wins over If-Modified-Since (RFC 9110 section 13.2.2).
    let not_modified = match req.header("if-none-match") {
        Some(inm) => {
            inm.trim() == "*"
                || inm
                    .split(',')
                    .any(|t| t.trim().trim_start_matches("W/") == etag)
        }
        None => req.header("if-modified-since") == Some(last_modified.as_str()),
    };
    if not_modified {
        return base(Response::new(304));
    }
    let ct = content_type(&canon);
    // Range is defined for GET only (RFC 9110 section 14.2): HEAD describes the whole file.
    let range = req.header("range").filter(|_| req.method == "GET");
    match range.map(|h| parse_range(h, data.len())) {
        Some(RangeSpec::Bytes(s, e)) => base(Response::new(206))
            .header("Content-Type", ct)
            .header("Content-Range", format!("bytes {s}-{e}/{}", data.len()))
            .bytes(data[s..=e].to_vec()),
        Some(RangeSpec::Unsatisfiable) => base(Response::new(416))
            .header("Content-Range", format!("bytes */{}", data.len()))
            .header("Content-Type", "text/plain")
            .bytes(b"416 range not satisfiable\n".to_vec()),
        _ => base(Response::new(200))
            .header("Content-Type", ct)
            .bytes(data),
    }
}

// ======================================================================== application: routes and middleware

/// A gate that handlers can block on (used to saturate the pool deterministically in tests).
struct Gate {
    open: Mutex<bool>,
    cv: Condvar,
    entered: AtomicUsize,
}

impl Gate {
    fn new() -> Gate {
        Gate {
            open: Mutex::new(false),
            cv: Condvar::new(),
            entered: AtomicUsize::new(0),
        }
    }
    fn wait(&self) {
        self.entered.fetch_add(1, SeqCst);
        let g = self.open.lock().unwrap();
        let _ = self
            .cv
            .wait_timeout_while(g, Duration::from_secs(10), |o| !*o)
            .unwrap(); // never blocks forever
    }
    fn open(&self) {
        *self.open.lock().unwrap() = true;
        self.cv.notify_all();
    }
}

struct Ctl {
    gate: Gate,
    slow_started: Mutex<mpsc::Sender<()>>,
    log: Mutex<VecDeque<String>>,
    next_id: AtomicU64,
}

fn build_app(site: &Path, ctl: Arc<Ctl>) -> App {
    let metrics = Arc::new(Metrics::new());
    let mut r = Router::default();
    r.add("GET", "/", |_, _| {
        Response::text(
            200,
            "mini-http: try /hello /echo/:word /static/index.txt /metrics\n",
        )
    });
    r.add("GET", "/hello", |_, _| Response::text(200, "hello"));
    r.add("GET", "/echo/:word", |_, p| {
        Response::text(200, format!("echo:{}", p.get("word")))
    });
    r.add("GET", "/users/:id/posts/:post", |_, p| {
        Response::text(200, format!("user={} post={}", p.get("id"), p.get("post")))
    });
    r.add("POST", "/echo-body", |req, _| {
        Response::new(200)
            .header("Content-Type", "application/octet-stream")
            .header("X-Body-Len", req.body.len().to_string())
            .bytes(req.body.clone())
    });
    r.add("POST", "/upload", |req, _| {
        Response::text(200, format!("received {} bytes", req.body.len()))
    });
    r.add("GET", "/stream/:n", |_, p| {
        let n: usize = p.get("n").parse().unwrap_or(0).min(1000);
        Response::new(200)
            .header("Content-Type", "text/plain")
            .body_stream((0..n).map(|i| format!("line {i}\n").into_bytes()))
    });
    let root = fs::canonicalize(site).expect("site dir");
    r.add("GET", "/static/*path", move |req, p| {
        serve_static(&root, p.get("path"), req)
    });
    let m = metrics.clone();
    r.add("GET", "/metrics", move |_, _| {
        Response::new(200)
            .header("Content-Type", "text/plain; version=0.0.4")
            .bytes(m.render().into_bytes())
    });
    r.add("GET", "/panic", |_, _| panic!("handler bug (intentional)"));
    let c = ctl.clone();
    r.add("GET", "/block", move |_, _| {
        c.gate.wait();
        Response::text(200, "released")
    });
    let c = ctl.clone();
    r.add("GET", "/slow", move |_, _| {
        let _ = c.slow_started.lock().unwrap().send(());
        thread::sleep(Duration::from_millis(150));
        Response::text(200, "slow done")
    });

    let c = ctl.clone();
    let request_id: Middleware = Arc::new(move |req, next| {
        let id = match req.header("x-request-id") {
            Some(v)
                if !v.is_empty()
                    && v.len() <= 64
                    && v.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') =>
            {
                v.to_string()
            }
            _ => format!("req-{:06}", c.next_id.fetch_add(1, SeqCst)),
        };
        next(req).header("X-Request-Id", id)
    });
    let c = ctl.clone();
    let logging: Middleware = Arc::new(move |req, next| {
        let t = Instant::now();
        let resp = next(req);
        let mut log = c.log.lock().unwrap();
        if log.len() == 64 {
            log.pop_front();
        }
        log.push_back(format!(
            "{} {} -> {} ({} us)",
            req.method,
            req.path,
            resp.status,
            t.elapsed().as_micros()
        ));
        resp
    });
    let m = metrics.clone();
    let timing: Middleware = Arc::new(move |req, next| {
        let t = Instant::now();
        let resp = next(req);
        let us = t.elapsed().as_micros() as u64;
        m.latency.record(us);
        resp.header(
            "Server-Timing",
            format!("app;dur={:.3}", us as f64 / 1000.0),
        )
    });
    let m = metrics.clone();
    let catch_panic: Middleware =
        Arc::new(
            move |req, next| match catch_unwind(AssertUnwindSafe(|| next(req))) {
                Ok(r) => r,
                Err(_) => {
                    m.handler_panics.fetch_add(1, SeqCst);
                    Response::text(500, "500 Internal Server Error\n")
                }
            },
        );
    App {
        router: r,
        mws: vec![request_id, logging, timing, catch_panic],
        metrics,
    }
}

impl Response {
    fn body_stream(mut self, it: impl Iterator<Item = Vec<u8>> + Send + 'static) -> Response {
        self.body = Body::Stream(Box::new(it));
        self
    }
}

// ======================================================================== the server

#[derive(Clone, Copy, PartialEq, Debug)]
enum Mode {
    Pool,
    PerConn,
}

#[derive(Clone)]
struct Config {
    workers: usize,
    queue_cap: usize,
    idle_timeout: Duration,
    request_timeout: Duration,
    max_requests_per_conn: usize,
    reject_linger: Duration,
    limits: Limits,
    mode: Mode,
}

fn test_cfg() -> Config {
    Config {
        workers: 8,
        queue_cap: 16,
        idle_timeout: Duration::from_secs(5),
        request_timeout: Duration::from_secs(2),
        max_requests_per_conn: 1000,
        reject_linger: Duration::from_millis(500),
        limits: Limits {
            max_request_line: 2048,
            max_header_bytes: 8192,
            max_headers: 64,
            max_body: 64 * 1024,
        },
        mode: Mode::Pool,
    }
}

struct Shared {
    app: App,
    cfg: Config,
    stopping: AtomicBool,
    live: AtomicUsize, // threads of this server that are currently running
}

struct LiveGuard<'a>(&'a AtomicUsize);

impl<'a> LiveGuard<'a> {
    fn new(c: &'a AtomicUsize) -> Self {
        c.fetch_add(1, SeqCst);
        LiveGuard(c)
    }
}

impl Drop for LiveGuard<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, SeqCst);
    }
}

struct Server {
    addr: SocketAddr,
    shared: Arc<Shared>,
    acceptor: Option<JoinHandle<()>>,
    workers: Vec<JoinHandle<()>>,
}

struct ShutdownReport {
    workers_joined: usize,
    elapsed: Duration,
    live_threads: usize,
}

impl Server {
    fn start(cfg: Config, app: App) -> io::Result<Server> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;
        let shared = Arc::new(Shared {
            app,
            cfg: cfg.clone(),
            stopping: AtomicBool::new(false),
            live: AtomicUsize::new(0),
        });
        let (tx, rx) = sync_channel::<TcpStream>(cfg.queue_cap);
        let rx = Arc::new(Mutex::new(rx));
        let mut workers = Vec::new();
        if cfg.mode == Mode::Pool {
            for i in 0..cfg.workers {
                let (sh, rx) = (shared.clone(), rx.clone());
                workers.push(
                    thread::Builder::new()
                        .name(format!("worker-{i}"))
                        .spawn(move || worker_loop(sh, rx))?,
                );
            }
        }
        let sh = shared.clone();
        let acceptor = thread::Builder::new()
            .name("acceptor".into())
            .spawn(move || acceptor_loop(listener, sh, tx))?;
        Ok(Server {
            addr,
            shared,
            acceptor: Some(acceptor),
            workers,
        })
    }

    fn metrics(&self) -> &Metrics {
        &self.shared.app.metrics
    }

    /// Stop accepting, finish in-flight requests, close idle connections, join every thread.
    fn shutdown(mut self) -> ShutdownReport {
        let t = Instant::now();
        self.shared.stopping.store(true, SeqCst);
        let _ = TcpStream::connect_timeout(&self.addr, Duration::from_secs(1)); // wakes the blocked accept()
        self.acceptor
            .take()
            .expect("acceptor")
            .join()
            .expect("acceptor thread panicked");
        let n = self.workers.len();
        for w in self.workers.drain(..) {
            w.join().expect("worker thread panicked");
        }
        ShutdownReport {
            workers_joined: n,
            elapsed: t.elapsed(),
            live_threads: self.shared.live.load(SeqCst),
        }
    }
}

fn acceptor_loop(listener: TcpListener, sh: Arc<Shared>, tx: SyncSender<TcpStream>) {
    let _live = LiveGuard::new(&sh.live);
    let m = &sh.app.metrics;
    let mut conn_threads: Vec<JoinHandle<()>> = Vec::new();
    let mut n = 0u64;
    let (refused_tx, refused_rx) = sync_channel::<TcpStream>(MAX_REFUSED_OPEN);
    let (sh2, linger) = (sh.clone(), sh.cfg.reject_linger);
    let rejector = thread::Builder::new()
        .name("rejector".into())
        .spawn(move || {
            let _live = LiveGuard::new(&sh2.live);
            reject_loop(refused_rx, linger);
        })
        .expect("spawn rejector");
    loop {
        let stream = match listener.accept() {
            Ok((s, _)) => s,
            Err(_) if sh.stopping.load(SeqCst) => break,
            Err(_) => {
                thread::sleep(Duration::from_millis(2)); // e.g. EMFILE: do not spin
                continue;
            }
        };
        if sh.stopping.load(SeqCst) {
            break; // the wake-up connection (or a late client): dropped, not served
        }
        m.accepted.fetch_add(1, SeqCst);
        let _ = stream.set_nonblocking(false); // BSD/macOS accepted sockets can inherit O_NONBLOCK
        let _ = stream.set_nodelay(true);
        match sh.cfg.mode {
            Mode::Pool => {
                m.queued.fetch_add(1, SeqCst);
                match tx.try_send(stream) {
                    Ok(()) => {}
                    Err(TrySendError::Full(s)) => {
                        m.queued.fetch_sub(1, SeqCst);
                        m.rejected.fetch_add(1, SeqCst);
                        m.count_response(503);
                        if let Some(s) = reject_503(s) {
                            let _ = refused_tx.try_send(s); // rejector far behind: drop the socket now
                        }
                    }
                    Err(TrySendError::Disconnected(_)) => {
                        m.queued.fetch_sub(1, SeqCst);
                    }
                }
            }
            Mode::PerConn => {
                n += 1;
                let sh2 = sh.clone();
                if let Ok(h) = thread::Builder::new()
                    .name(format!("conn-{n}"))
                    .spawn(move || {
                        let _live = LiveGuard::new(&sh2.live);
                        let _ = catch_unwind(AssertUnwindSafe(|| serve_connection(&sh2, stream)));
                    })
                {
                    conn_threads.push(h);
                }
                if conn_threads.len() > 256 {
                    conn_threads.retain(|h| !h.is_finished());
                }
            }
        }
    }
    drop(tx); // workers drain the queue, then see Disconnected and exit
    drop(refused_tx);
    for h in conn_threads {
        let _ = h.join();
    }
    let _ = rejector.join();
}

/// Load shedding: tell the client to come back later instead of letting it wait in an ever-growing queue.
/// Runs on the acceptor, so it never waits for the client; the socket then goes to `reject_loop` to be closed.
fn reject_503(mut s: TcpStream) -> Option<TcpStream> {
    let _ = s.set_write_timeout(Some(Duration::from_millis(200)));
    let body = "503 Service Unavailable: server busy, retry shortly\n";
    let msg = format!(
        "HTTP/1.1 503 Service Unavailable\r\nRetry-After: 1\r\nConnection: close\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    s.write_all(msg.as_bytes()).ok()?;
    s.shutdown(Shutdown::Write).ok()?;
    s.set_nonblocking(true).ok()?;
    Some(s)
}

/// Most refused sockets held open at once; beyond it the oldest is dropped early.
const MAX_REFUSED_OPEN: usize = 64;

/// One thread closes all refused sockets. Closing while input is unread, or before late input arrives, sends RST,
/// which can destroy the 503. So each socket is read (without blocking) until the client closes, or for `linger`.
fn reject_loop(rx: Receiver<TcpStream>, linger: Duration) {
    let mut open: VecDeque<(TcpStream, Instant, usize)> = VecDeque::new();
    let mut sink = [0u8; 4096];
    let mut accepting = true;
    while accepting || !open.is_empty() {
        let next = if open.is_empty() {
            rx.recv().map_err(|_| mpsc::RecvTimeoutError::Disconnected)
        } else {
            rx.recv_timeout(Duration::from_millis(5))
        };
        match next {
            Ok(s) => {
                if open.len() == MAX_REFUSED_OPEN {
                    open.pop_front();
                }
                open.push_back((s, Instant::now(), 0));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                accepting = false;
                thread::sleep(Duration::from_millis(5)); // recv no longer waits: do not spin while the rest close
            }
        }
        open.retain_mut(|(s, since, total)| {
            let alive = match s.read(&mut sink) {
                Ok(0) => false, // the client closed: nothing can be lost now
                Ok(n) => {
                    *total += n;
                    *total <= 64 * 1024
                }
                Err(e) => matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted),
            };
            alive && since.elapsed() < linger
        });
    }
}

fn worker_loop(sh: Arc<Shared>, rx: Arc<Mutex<Receiver<TcpStream>>>) {
    let _live = LiveGuard::new(&sh.live);
    loop {
        let job = rx.lock().unwrap().recv(); // idle workers queue on the mutex; the guard is a temporary
        let Ok(stream) = job else { break };
        sh.app.metrics.queued.fetch_sub(1, SeqCst);
        if catch_unwind(AssertUnwindSafe(|| serve_connection(&sh, stream))).is_err() {
            sh.app.metrics.worker_panics.fetch_add(1, SeqCst); // second line of defence: the worker lives on
        }
    }
}

/// Longest a worker spends on a lingering close, however steadily the peer keeps sending.
const LINGER_TOTAL: Duration = Duration::from_millis(500);

/// Close politely: send FIN, then read and discard what the peer still sends until it closes, goes quiet for
/// 50 ms, has sent 64 KiB or `LINGER_TOTAL` has passed.
fn lingering_close(mut s: TcpStream) {
    let _ = s.shutdown(Shutdown::Write);
    let _ = s.set_read_timeout(Some(Duration::from_millis(50)));
    let began = Instant::now();
    let mut sink = [0u8; 4096];
    let mut total = 0;
    while began.elapsed() < LINGER_TOTAL {
        let Ok(n) = s.read(&mut sink) else { break };
        total += n;
        if n == 0 || total > 64 * 1024 {
            break;
        }
    }
}

fn serve_connection(sh: &Shared, mut stream: TcpStream) {
    let m = &sh.app.metrics;
    m.active.fetch_add(1, SeqCst);
    struct Active<'a>(&'a AtomicI64);
    impl Drop for Active<'_> {
        fn drop(&mut self) {
            self.0.fetch_sub(1, SeqCst);
        }
    }
    let _active = Active(&m.active);
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_read_timeout(Some(Duration::from_millis(50)));
    let (mut buf, mut tmp) = (Vec::<u8>::with_capacity(4096), [0u8; 8192]);
    let (mut served, mut sent_continue) = (0usize, false);
    let mut last_activity = Instant::now();
    let mut partial_since: Option<Instant> = None;
    loop {
        if !buf.is_empty() {
            match parse_request(&buf, &sh.cfg.limits) {
                Parsed::Done(req, used) => {
                    buf.drain(..used); // whatever is left is the next pipelined request
                    (sent_continue, partial_since) = (false, None);
                    served += 1;
                    if served > 1 {
                        m.reused.fetch_add(1, SeqCst);
                    }
                    let resp = sh.app.handle(&req);
                    let keep = wants_keep_alive(&req)
                        && can_keep_alive(req.minor, &resp)
                        && served < sh.cfg.max_requests_per_conn
                        && !sh.stopping.load(SeqCst);
                    m.count_response(resp.status); // counted before the write: a client that has the response sees it counted
                    let head = req.method == "HEAD";
                    match write_response(&mut stream, resp, head, keep, req.minor) {
                        Ok(n) => m.bytes_out.fetch_add(n, SeqCst),
                        Err(_) => return,
                    };
                    if !keep {
                        lingering_close(stream);
                        return;
                    }
                    last_activity = Instant::now();
                    continue;
                }
                Parsed::Bad(e) => {
                    let resp = error_response(&e);
                    m.count_response(resp.status);
                    let _ = write_response(&mut stream, resp, false, false, 1);
                    lingering_close(stream);
                    return;
                }
                Parsed::Incomplete { expect_continue } => {
                    // Checked on every turn, not only after a silent read: a client that sends one byte
                    // just often enough would otherwise hold the worker (and shutdown) for ever.
                    let since = *partial_since.get_or_insert_with(Instant::now);
                    if since.elapsed() >= sh.cfg.request_timeout {
                        let resp = Response::text(408, "408 Request Timeout\n");
                        m.count_response(408);
                        let _ = write_response(&mut stream, resp, false, false, 1);
                        lingering_close(stream);
                        return;
                    }
                    if expect_continue && !sent_continue {
                        sent_continue = true;
                        if stream.write_all(b"HTTP/1.1 100 Continue\r\n\r\n").is_err() {
                            return;
                        }
                    }
                }
            }
        }
        match stream.read(&mut tmp) {
            Ok(0) => return, // peer closed
            Ok(n) => {
                buf.extend_from_slice(&tmp[..n]);
                last_activity = Instant::now();
            }
            Err(e)
                if matches!(
                    e.kind(),
                    ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted
                ) =>
            {
                if buf.is_empty()
                    && (sh.stopping.load(SeqCst) || last_activity.elapsed() >= sh.cfg.idle_timeout)
                {
                    m.idle_closed.fetch_add(1, SeqCst);
                    return; // idle keep-alive connection: close it
                }
            }
            Err(_) => return,
        }
    }
}

// ======================================================================== tokio comparison server (same parser, router, writer)

struct TokioServer {
    addr: SocketAddr,
    rt: tokio::runtime::Runtime,
    app: Arc<App>,
}

fn start_tokio(app: App, limits: Limits) -> TokioServer {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .expect("runtime");
    let std_l = TcpListener::bind("127.0.0.1:0").expect("bind");
    std_l.set_nonblocking(true).unwrap();
    let addr = std_l.local_addr().unwrap();
    let app = Arc::new(app);
    let a2 = app.clone();
    rt.spawn(async move {
        let l = tokio::net::TcpListener::from_std(std_l).expect("listener");
        while let Ok((s, _)) = l.accept().await {
            a2.metrics.accepted.fetch_add(1, SeqCst);
            tokio::spawn(tokio_conn(s, a2.clone(), limits.clone()));
        }
    });
    TokioServer { addr, rt, app }
}

async fn tokio_conn(mut s: tokio::net::TcpStream, app: Arc<App>, lim: Limits) {
    let _ = s.set_nodelay(true);
    let (mut buf, mut tmp) = (Vec::<u8>::new(), [0u8; 8192]);
    loop {
        loop {
            match parse_request(&buf, &lim) {
                Parsed::Done(req, used) => {
                    buf.drain(..used);
                    let resp = app.handle(&req);
                    let keep = wants_keep_alive(&req) && can_keep_alive(req.minor, &resp);
                    let mut out = Vec::new();
                    let _ = write_response(&mut out, resp, req.method == "HEAD", keep, req.minor);
                    if s.write_all(&out).await.is_err() || !keep {
                        return;
                    }
                }
                Parsed::Bad(e) => {
                    let mut out = Vec::new();
                    let _ = write_response(&mut out, error_response(&e), false, false, 1);
                    let _ = s.write_all(&out).await;
                    return;
                }
                Parsed::Incomplete { .. } => break,
            }
        }
        match tokio::time::timeout(Duration::from_secs(5), s.read(&mut tmp)).await {
            Ok(Ok(n)) if n > 0 => buf.extend_from_slice(&tmp[..n]),
            _ => return,
        }
    }
}

// ======================================================================== test client + load generator

struct Resp {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Resp {
    fn header(&self, n: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k == n)
            .map(|(_, v)| v.as_str())
    }
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

/// Blocking client with persistent connection, leftover-byte buffer (pipelining) and a 5 s timeout on every read/write.
struct Client {
    s: TcpStream,
    buf: Vec<u8>,
}

impl Client {
    fn connect(addr: SocketAddr) -> Client {
        let s = TcpStream::connect_timeout(&addr, Duration::from_secs(5)).expect("connect");
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        s.set_write_timeout(Some(Duration::from_secs(5))).unwrap();
        s.set_nodelay(true).unwrap();
        Client { s, buf: Vec::new() }
    }
    fn send(&mut self, b: &[u8]) {
        self.s.write_all(b).expect("client write");
    }
    fn fill(&mut self) -> io::Result<()> {
        let mut tmp = [0u8; 16384];
        match self.s.read(&mut tmp)? {
            0 => Err(io::Error::new(ErrorKind::UnexpectedEof, "closed")),
            n => {
                self.buf.extend_from_slice(&tmp[..n]);
                Ok(())
            }
        }
    }
    fn take_line(&mut self) -> io::Result<String> {
        loop {
            if let Some(i) = find(&self.buf, b"\r\n") {
                let l = String::from_utf8_lossy(&self.buf[..i]).into_owned();
                self.buf.drain(..i + 2);
                return Ok(l);
            }
            self.fill()?;
        }
    }
    fn take_n(&mut self, n: usize) -> io::Result<Vec<u8>> {
        while self.buf.len() < n {
            self.fill()?;
        }
        Ok(self.buf.drain(..n).collect())
    }
    fn read_response(&mut self, head_only: bool) -> io::Result<Resp> {
        let bad = |m: &str| io::Error::new(ErrorKind::InvalidData, m.to_string());
        let end = loop {
            if let Some(i) = find(&self.buf, b"\r\n\r\n") {
                break i;
            }
            self.fill()?;
        };
        let head = String::from_utf8_lossy(&self.buf[..end]).into_owned();
        self.buf.drain(..end + 4);
        let mut lines = head.split("\r\n");
        let status: u16 = lines
            .next()
            .and_then(|l| l.split(' ').nth(1))
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| bad("status line"))?;
        let headers = lines
            .filter_map(|l| {
                l.split_once(':')
                    .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
            })
            .collect();
        let mut r = Resp {
            status,
            headers,
            body: Vec::new(),
        };
        if head_only || status / 100 == 1 || status == 204 || status == 304 {
            return Ok(r);
        }
        if r.header("transfer-encoding") == Some("chunked") {
            loop {
                let size = usize::from_str_radix(self.take_line()?.trim(), 16)
                    .map_err(|_| bad("chunk size"))?;
                if size == 0 {
                    self.take_line()?;
                    break;
                }
                r.body.extend(self.take_n(size)?);
                self.take_line()?;
            }
        } else if let Some(cl) = r.header("content-length") {
            let n: usize = cl.parse().map_err(|_| bad("content-length"))?;
            r.body = self.take_n(n)?;
        }
        Ok(r)
    }
    fn request(&mut self, method: &str, path: &str, headers: &[(&str, &str)], body: &[u8]) -> Resp {
        let mut req = format!("{method} {path} HTTP/1.1\r\nHost: test\r\n");
        for (k, v) in headers {
            req.push_str(&format!("{k}: {v}\r\n"));
        }
        if !body.is_empty() || method == "POST" {
            req.push_str(&format!("Content-Length: {}\r\n", body.len()));
        }
        req.push_str("\r\n");
        let mut bytes = req.into_bytes();
        bytes.extend_from_slice(body);
        self.send(&bytes);
        self.read_response(method == "HEAD")
            .unwrap_or_else(|e| panic!("{method} {path}: {e}"))
    }
    fn get(&mut self, path: &str) -> Resp {
        self.request("GET", path, &[], b"")
    }
    /// True when the server closes the connection (EOF) within the read timeout.
    fn closed_by_peer(&mut self) -> bool {
        matches!(self.fill(), Err(e) if e.kind() == ErrorKind::UnexpectedEof || e.kind() == ErrorKind::ConnectionReset)
    }
}

struct LoadReport {
    name: &'static str,
    total: u64,
    ok: u64,
    secs: f64,
    hist: Histogram,
}

impl LoadReport {
    fn rps(&self) -> f64 {
        self.ok as f64 / self.secs
    }
}

/// `clients` threads, each with ONE persistent connection, each sending `per_client` GET /echo/<unique> and checking the body.
fn load_test(
    name: &'static str,
    addr: SocketAddr,
    clients: usize,
    per_client: usize,
) -> LoadReport {
    let hist = Arc::new(Histogram::new());
    let ok = Arc::new(AtomicU64::new(0));
    let barrier = Arc::new(Barrier::new(clients + 1));
    let handles: Vec<_> = (0..clients)
        .map(|c| {
            let (hist, ok, barrier) = (hist.clone(), ok.clone(), barrier.clone());
            thread::spawn(move || {
                let mut cl = Client::connect(addr);
                barrier.wait();
                for i in 0..per_client {
                    let word = format!("c{c}-{i}");
                    let t = Instant::now();
                    let r = cl.request("GET", &format!("/echo/{word}"), &[], b"");
                    hist.record(t.elapsed().as_micros() as u64);
                    if r.status == 200 && r.body == format!("echo:{word}").as_bytes() {
                        ok.fetch_add(1, SeqCst);
                    }
                }
            })
        })
        .collect();
    barrier.wait();
    let t = Instant::now();
    for h in handles {
        h.join().expect("client thread");
    }
    let secs = t.elapsed().as_secs_f64();
    let hist = Arc::try_unwrap(hist).ok().expect("histogram still shared");
    LoadReport {
        name,
        total: (clients * per_client) as u64,
        ok: ok.load(SeqCst),
        secs,
        hist,
    }
}

// ======================================================================== main: demo + assertions

fn check(name: &str, cond: bool) {
    println!("  [{}] {name}", if cond { "ok" } else { "FAIL" });
    assert!(cond, "check failed: {name}");
}

fn wait_until(what: &str, f: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !f() {
        assert!(Instant::now() < deadline, "timed out waiting for: {what}");
        thread::sleep(Duration::from_millis(2));
    }
}

/// What the parser says about `raw` (200 = one whole request, else the error status), provided no earlier cut of
/// the same bytes said something different: every shorter prefix is Incomplete or already the same refusal.
/// `None` means the answer depends on where the network cut the bytes.
fn verdict_at_every_cut(raw: &[u8], lim: &Limits) -> Option<u16> {
    let whole = match parse_request(raw, lim) {
        Parsed::Done(_, used) if used == raw.len() => 200,
        Parsed::Bad(e) => e.status(),
        _ => return None,
    };
    (0..raw.len())
        .all(|cut| match parse_request(&raw[..cut], lim) {
            Parsed::Incomplete { .. } => true,
            Parsed::Bad(e) => e.status() == whole,
            Parsed::Done(..) => false,
        })
        .then_some(whole)
}

/// Sends one byte about every 10 ms, collecting whatever arrives, until `done` is true. The 5 s cap is many times
/// longer than any server timeout involved, so it is reached only when the server never acts.
fn drip_until(c: &mut Client, got: &mut Vec<u8>, done: impl Fn(&[u8]) -> bool) -> bool {
    c.s.set_nonblocking(true).unwrap();
    let began = Instant::now();
    let mut tmp = [0u8; 512];
    while began.elapsed() < Duration::from_secs(5) {
        if done(got) {
            return true;
        }
        let _ = c.s.write(b"a"); // fails once the server has gone, which is fine
        thread::sleep(Duration::from_millis(10));
        if let Ok(n) = c.s.read(&mut tmp) {
            got.extend_from_slice(&tmp[..n]);
        }
    }
    false
}

fn make_site() -> (PathBuf, PathBuf) {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let base = std::env::temp_dir().join(format!("mini_http_{}_{nanos}", std::process::id()));
    let public = base.join("public");
    fs::create_dir_all(public.join("sub")).unwrap();
    fs::write(public.join("index.txt"), "hello from a static file\n").unwrap();
    fs::write(public.join("sub/nested.txt"), "nested\n").unwrap();
    let big: Vec<u8> = (0..1000u32).map(|i| (i % 251) as u8).collect();
    fs::write(public.join("big.bin"), &big).unwrap();
    fs::write(base.join("secret.txt"), "TOP SECRET OUTSIDE THE ROOT\n").unwrap();
    std::os::unix::fs::symlink(base.join("secret.txt"), public.join("escape.txt")).unwrap();
    (base, public)
}

fn new_ctl() -> (Arc<Ctl>, mpsc::Receiver<()>) {
    let (tx, rx) = mpsc::channel();
    (
        Arc::new(Ctl {
            gate: Gate::new(),
            slow_started: Mutex::new(tx),
            log: Mutex::new(VecDeque::new()),
            next_id: AtomicU64::new(1),
        }),
        rx,
    )
}

fn main() {
    println!(
        "=== mini HTTP/1.1 server on std::net: thread pool, keep-alive, pipelining, router, static files ==="
    );
    // Intentional handler panics would print noisy backtraces from worker threads; keep the default hook for everything else.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let name = thread::current().name().unwrap_or("").to_string();
        if name.starts_with("worker-") || name.starts_with("conn-") {
            eprintln!("  (caught panic on {name}: {info})");
        } else {
            default_hook(info);
        }
    }));

    // ---- 0. unit checks of the building blocks
    println!("\n[0] building blocks");
    let h = Histogram::new();
    for v in 1..=10_000u64 {
        h.record(v);
    }
    let (p50, p99) = (h.quantile(0.5), h.quantile(0.99));
    println!(
        "  histogram of 1..=10000 us: p50={p50} p90={} p99={p99} max={}",
        h.quantile(0.9),
        h.quantile(1.0)
    );
    check(
        "histogram p50 within 12.5% of 5000",
        (4375..=5625).contains(&p50),
    );
    check(
        "histogram p99 within 12.5% of 9900",
        (8660..=11_137).contains(&p99),
    );
    check(
        "http_date(0) and a known timestamp",
        http_date(UNIX_EPOCH) == "Thu, 01 Jan 1970 00:00:00 GMT"
            && http_date(UNIX_EPOCH + Duration::from_secs(1_700_000_000))
                == "Tue, 14 Nov 2023 22:13:20 GMT",
    );
    let lim = test_cfg().limits;
    check(
        "parser: partial request is Incomplete",
        matches!(
            parse_request(b"GET / HTTP/1.1\r\nHost: a\r\n", &lim),
            Parsed::Incomplete { .. }
        ),
    );
    let two = b"GET /a HTTP/1.1\r\nHost: h\r\n\r\nGET /b HTTP/1.1\r\nHost: h\r\n\r\n";
    check(
        "parser: Done reports bytes used (pipelining)",
        matches!(parse_request(two, &lim), Parsed::Done(r, 28) if r.path == "/a"),
    );
    check("parser: Content-Length + chunked is rejected", matches!(parse_request(b"POST / HTTP/1.1\r\nHost: h\r\nContent-Length: 1\r\nTransfer-Encoding: chunked\r\n\r\n", &lim), Parsed::Bad(HttpError::BadRequest(_))));
    for cut in 0..two.len() {
        // every prefix of a valid request must be Incomplete or Done, never Bad and never a panic
        assert!(
            !matches!(parse_request(&two[..cut], &lim), Parsed::Bad(_)),
            "prefix {cut}"
        );
    }
    check("parser: no valid prefix is ever 'Bad'", true);

    let request_line = |n: usize| {
        let target = "a".repeat(n - "GET / HTTP/1.1".len());
        format!("GET /{target} HTTP/1.1\r\nHost: h\r\n\r\n").into_bytes()
    };
    let max = lim.max_request_line;
    check(
        "request-line limit does not depend on where the bytes are cut",
        verdict_at_every_cut(&request_line(max), &lim) == Some(200)
            && verdict_at_every_cut(&request_line(max + 1), &lim) == Some(414),
    );
    const CHUNKED: &str = "POST / HTTP/1.1\r\nHost: h\r\nTransfer-Encoding: chunked\r\n\r\n";
    let chunk_line =
        |n: usize| format!("{CHUNKED}1;{}\r\nz\r\n0\r\n\r\n", "e".repeat(n - 2)).into_bytes();
    check(
        "chunk-line limit does not depend on where the bytes are cut",
        verdict_at_every_cut(&chunk_line(MAX_CHUNK_LINE), &lim) == Some(200)
            && verdict_at_every_cut(&chunk_line(MAX_CHUNK_LINE + 1), &lim) == Some(400),
    );
    let small = Limits {
        max_header_bytes: 128,
        ..lim.clone()
    };
    // `n` bytes of trailer section: one trailer line, then the empty line
    let trailers = |n: usize| {
        format!(
            "{CHUNKED}0\r\nT: {}\r\n\r\n",
            "t".repeat(n - "T: \r\n\r\n".len())
        )
        .into_bytes()
    };
    let many_trailers = format!("{CHUNKED}0\r\n{}", "T: x\r\n".repeat(100)).into_bytes();
    let endless_trailer = format!("{CHUNKED}0\r\n{}", "t".repeat(1000)).into_bytes();
    check(
        "the trailer section is bounded, finished or not",
        verdict_at_every_cut(&trailers(128), &small) == Some(200)
            && verdict_at_every_cut(&trailers(129), &small) == Some(431)
            && verdict_at_every_cut(&many_trailers, &small) == Some(431)
            && verdict_at_every_cut(&endless_trailer, &small) == Some(431),
    );
    for second in ["gzip", "chunked", "identity"] {
        let raw = format!(
            "POST / HTTP/1.1\r\nHost: h\r\nTransfer-Encoding: chunked\r\nTransfer-Encoding: {second}\r\n\r\n0\r\n\r\n"
        );
        assert!(
            matches!(
                parse_request(raw.as_bytes(), &lim),
                Parsed::Bad(HttpError::BadRequest(_))
            ),
            "second Transfer-Encoding: {second}"
        );
    }
    check("parser: a repeated Transfer-Encoding is rejected", true);
    check(
        "percent decoding takes exactly two hex digits",
        pct_decode("%2e%2E%41").as_deref() == Some("..A")
            && ["a%+5", "%-0", "% 5", "%4", "%", "%g0", "%ff"]
                .iter()
                .all(|bad| pct_decode(bad).is_none()),
    );
    let m = Metrics::new();
    for le in LE_US {
        m.latency.record(le);
        m.latency.record(le + 1);
    }
    let report = m.render();
    check(
        "Prometheus le buckets include a sample equal to the bound, and not the next value",
        LE_US.iter().enumerate().all(|(i, le)| {
            let line = format!(
                "http_request_duration_us_bucket{{le=\"{le}\"}} {}\n",
                2 * i + 1
            );
            report.contains(&line)
        }),
    );
    let covered = (0..64)
        .flat_map(|e| [(1u64 << e) - 1, 1 << e, (1 << e) + 1])
        .chain([u64::MAX])
        .all(|v| {
            let top = Histogram::upper(Histogram::index(v));
            top >= v && top - v <= v / 8
        });
    let huge = Histogram::new();
    huge.record(1 << 50);
    huge.record(u64::MAX);
    check(
        "histogram covers all of u64 within 12.5%",
        covered && ((1u64 << 50)..=(9 << 47)).contains(&huge.quantile(0.5)),
    );

    let (base, public) = make_site();
    let (ctl, slow_rx) = new_ctl();

    // ---- 1. routes, middleware, HEAD
    println!("\n[1] routes and middleware (8 workers, queue 16)");
    let srv = Server::start(test_cfg(), build_app(&public, ctl.clone())).expect("start");
    let mut c = Client::connect(srv.addr);
    let r = c.get("/hello");
    println!(
        "  GET /hello -> {} {:?} X-Request-Id={:?} Server-Timing={:?}",
        r.status,
        r.text(),
        r.header("x-request-id"),
        r.header("server-timing")
    );
    check("GET /hello", r.status == 200 && r.text() == "hello");
    check(
        "request-id generated",
        r.header("x-request-id")
            .is_some_and(|v| v.starts_with("req-")),
    );
    check(
        "timing middleware adds Server-Timing",
        r.header("server-timing")
            .is_some_and(|v| v.starts_with("app;dur=")),
    );
    let r = c.request("GET", "/echo/xyz", &[("X-Request-Id", "abc-123")], b"");
    check(
        "request-id echoed when the client sent a sane one",
        r.header("x-request-id") == Some("abc-123") && r.text() == "echo:xyz",
    );
    let r = c.get("/users/7/posts/42");
    println!("  GET /users/7/posts/42 -> {:?}", r.text());
    check("path parameters", r.text() == "user=7 post=42");
    check("404 for unknown path", c.get("/nope").status == 404);
    let r = c.request("POST", "/hello", &[], b"x");
    println!(
        "  POST /hello -> {} Allow={:?}",
        r.status,
        r.header("allow")
    );
    check(
        "405 with Allow",
        r.status == 405 && r.header("allow") == Some("GET"),
    );
    let r = c.request("HEAD", "/hello", &[], b"");
    check(
        "HEAD: same headers, Content-Length 5, no body",
        r.status == 200 && r.header("content-length") == Some("5") && r.body.is_empty(),
    );
    check(
        "connection still in sync after HEAD (no stray body bytes)",
        c.get("/echo/after-head").text() == "echo:after-head",
    );
    println!(
        "  recent log lines: {:?}",
        ctl.log
            .lock()
            .unwrap()
            .iter()
            .rev()
            .take(2)
            .collect::<Vec<_>>()
    );

    // ---- 2. pipelining and incremental parsing
    println!("\n[2] pipelining and byte-at-a-time requests");
    let mut p = Client::connect(srv.addr);
    let burst = "GET /echo/one HTTP/1.1\r\nHost: t\r\n\r\nGET /echo/two HTTP/1.1\r\nHost: t\r\n\r\nHEAD /hello HTTP/1.1\r\nHost: t\r\n\r\n\
                 POST /upload HTTP/1.1\r\nHost: t\r\nContent-Length: 5\r\n\r\nhelloGET /echo/five HTTP/1.1\r\nHost: t\r\n\r\n";
    p.send(burst.as_bytes()); // five requests, ONE write
    let got: Vec<Resp> = [false, false, true, false, false]
        .iter()
        .map(|&h| p.read_response(h).expect("pipelined response"))
        .collect();
    println!(
        "  5 requests in one write -> {:?}",
        got.iter().map(|r| (r.status, r.text())).collect::<Vec<_>>()
    );
    check(
        "pipelined responses arrive in request order",
        got[0].text() == "echo:one"
            && got[1].text() == "echo:two"
            && got[2].body.is_empty()
            && got[3].text() == "received 5 bytes"
            && got[4].text() == "echo:five",
    );
    let slow_req = b"GET /echo/dribble HTTP/1.1\r\nHost: t\r\nX-Pad: 0123456789\r\n\r\n";
    for b in slow_req.iter() {
        p.send(&[*b]); // TCP_NODELAY on both sides: nearly one segment per byte
    }
    check(
        "request delivered one byte per write is parsed",
        p.read_response(false).unwrap().text() == "echo:dribble",
    );

    // ---- 3. bodies: content-length, chunked, 100-continue
    println!("\n[3] request bodies");
    let r = c.request("POST", "/echo-body", &[], b"plain body");
    check(
        "Content-Length body echoed",
        r.body == b"plain body" && r.header("x-body-len") == Some("10"),
    );
    c.send(b"POST /echo-body HTTP/1.1\r\nHost: t\r\nTransfer-Encoding: chunked\r\n\r\n");
    c.send(b"5\r\nhello\r\n");
    c.send(b"6;ext=1\r\n world\r\n");
    c.send(b"0\r\nTrailer: x\r\n\r\n");
    let r = c.read_response(false).unwrap();
    println!("  chunked upload (3 writes) -> {:?}", r.text());
    check(
        "chunked request body reassembled (with extension and trailer)",
        r.text() == "hello world",
    );
    c.send(
        b"POST /upload HTTP/1.1\r\nHost: t\r\nContent-Length: 11\r\nExpect: 100-continue\r\n\r\n",
    );
    let interim = c.read_response(false).unwrap();
    println!("  Expect: 100-continue -> interim {}", interim.status);
    check(
        "server sent 100 Continue before the body",
        interim.status == 100,
    );
    c.send(b"hello world");
    check(
        "final response after the body",
        c.read_response(false).unwrap().text() == "received 11 bytes",
    );

    // ---- 4. protocol errors
    println!("\n[4] limits and errors (each closes the connection)");
    let cases: Vec<(&str, Vec<u8>, u16)> = vec![
        ("garbage request line", b"BOGUS\r\n\r\n".to_vec(), 400),
        ("URI of 3000 bytes", format!("GET /{} HTTP/1.1\r\nHost: t\r\n\r\n", "a".repeat(3000)).into_bytes(), 414),
        ("one 9000-byte header", format!("GET / HTTP/1.1\r\nHost: t\r\nX-Big: {}\r\n\r\n", "b".repeat(9000)).into_bytes(), 431),
        ("100 headers", format!("GET / HTTP/1.1\r\nHost: t\r\n{}\r\n", (0..100).map(|i| format!("X-{i}: v\r\n")).collect::<String>()).into_bytes(), 431),
        ("Content-Length 10 MB (answered before any body)", b"POST /upload HTTP/1.1\r\nHost: t\r\nContent-Length: 10000000\r\n\r\n".to_vec(), 413),
        ("chunk bigger than the limit", b"POST /upload HTTP/1.1\r\nHost: t\r\nTransfer-Encoding: chunked\r\n\r\nFFFFFF\r\n".to_vec(), 413),
        ("smuggling attempt: CL + TE", b"POST /upload HTTP/1.1\r\nHost: t\r\nContent-Length: 3\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n".to_vec(), 400),
        ("HTTP/1.1 without Host", b"GET / HTTP/1.1\r\n\r\n".to_vec(), 400),
    ];
    for (name, bytes, want) in cases {
        let mut e = Client::connect(srv.addr);
        e.send(&bytes);
        let r = e.read_response(false).unwrap();
        println!("  {name:<52} -> {}", r.status);
        check(
            &format!("{name}: {want} and connection closed"),
            r.status == want && r.header("connection") == Some("close") && e.closed_by_peer(),
        );
    }

    // ---- 5. static files
    println!("\n[5] static files: ETag, Last-Modified, Range, traversal");
    let r = c.get("/static/index.txt");
    let etag = r.header("etag").unwrap().to_string();
    println!(
        "  GET /static/index.txt -> {} ETag={etag} Last-Modified={:?}",
        r.status,
        r.header("last-modified")
    );
    check(
        "static 200 with validators",
        r.status == 200
            && r.text() == "hello from a static file\n"
            && r.header("last-modified")
                .is_some_and(|d| d.ends_with("GMT")),
    );
    let r2 = c.request("GET", "/static/index.txt", &[("If-None-Match", &etag)], b"");
    check(
        "If-None-Match -> 304 with no body",
        r2.status == 304 && r2.body.is_empty(),
    );
    let r3 = c.request(
        "GET",
        "/static/index.txt",
        &[("If-None-Match", "\"stale\"")],
        b"",
    );
    check(
        "stale ETag -> full 200",
        r3.status == 200 && !r3.body.is_empty(),
    );
    let lm = r.header("last-modified").unwrap().to_string();
    check(
        "If-Modified-Since -> 304",
        c.request(
            "GET",
            "/static/index.txt",
            &[("If-Modified-Since", &lm)],
            b"",
        )
        .status
            == 304,
    );
    let big: Vec<u8> = (0..1000u32).map(|i| (i % 251) as u8).collect();
    let r = c.request("GET", "/static/big.bin", &[("Range", "bytes=0-9")], b"");
    println!(
        "  Range bytes=0-9 -> {} Content-Range={:?}",
        r.status,
        r.header("content-range")
    );
    check(
        "206 first ten bytes",
        r.status == 206
            && r.body == big[0..10]
            && r.header("content-range") == Some("bytes 0-9/1000"),
    );
    let r = c.request("GET", "/static/big.bin", &[("Range", "bytes=990-")], b"");
    check(
        "206 open-ended range",
        r.status == 206
            && r.body == big[990..]
            && r.header("content-range") == Some("bytes 990-999/1000"),
    );
    let r = c.request("GET", "/static/big.bin", &[("Range", "bytes=-10")], b"");
    check("206 suffix range", r.status == 206 && r.body == big[990..]);
    let r = c.request(
        "GET",
        "/static/big.bin",
        &[("Range", "bytes=500-5000")],
        b"",
    );
    check(
        "206 end clamped to file size",
        r.status == 206
            && r.body == big[500..]
            && r.header("content-range") == Some("bytes 500-999/1000"),
    );
    let r = c.request(
        "GET",
        "/static/big.bin",
        &[("Range", "bytes=2000-3000")],
        b"",
    );
    println!(
        "  Range bytes=2000-3000 -> {} Content-Range={:?}",
        r.status,
        r.header("content-range")
    );
    check(
        "416 with Content-Range: bytes */1000",
        r.status == 416 && r.header("content-range") == Some("bytes */1000"),
    );
    for range in ["bytes=0-9", "bytes=2000-3000"] {
        let r = c.request("HEAD", "/static/big.bin", &[("Range", range)], b"");
        check(
            &format!("HEAD ignores Range: {range}"),
            r.status == 200
                && r.header("content-length") == Some("1000")
                && r.header("content-range").is_none(),
        );
    }
    check(
        "full file without Range",
        c.get("/static/big.bin").body == big,
    );
    check(
        "nested file",
        c.get("/static/sub/nested.txt").text() == "nested\n",
    );
    check(
        "missing file 404",
        c.get("/static/missing.txt").status == 404,
    );
    for evil in [
        "/static/../secret.txt",
        "/static/sub/../../secret.txt",
        "/static/%2e%2e/secret.txt",
        "/static/..%2fsecret.txt",
        "/static/%2e%2e%2f%2e%2e%2fetc/passwd",
        "/static/sub%5c..%5c..%5csecret.txt",
        "/static/escape.txt",
    ] {
        let mut e = Client::connect(srv.addr);
        let r = e.get(evil);
        println!("  {evil:<44} -> {}", r.status);
        check(
            &format!("traversal blocked: {evil}"),
            (r.status == 403 || r.status == 404) && !r.text().contains("SECRET"),
        );
    }
    check(
        "symlink escaping the root is 403",
        Client::connect(srv.addr).get("/static/escape.txt").status == 403,
    );

    // ---- 6. streaming
    println!("\n[6] streaming chunked response");
    let r = c.get("/stream/5");
    println!(
        "  GET /stream/5 -> {} Transfer-Encoding={:?} body={:?}",
        r.status,
        r.header("transfer-encoding"),
        r.text()
    );
    check(
        "chunked response decoded",
        r.header("transfer-encoding") == Some("chunked")
            && r.header("content-length").is_none()
            && r.text() == "line 0\nline 1\nline 2\nline 3\nline 4\n",
    );
    check(
        "connection reusable after a chunked response",
        c.get("/hello").text() == "hello",
    );
    let mut old = Client::connect(srv.addr);
    old.send(b"GET /stream/3 HTTP/1.0\r\nConnection: keep-alive\r\n\r\n");
    let mut raw = Vec::new();
    let ended = old.s.read_to_end(&mut raw).is_ok();
    let raw = String::from_utf8_lossy(&raw);
    let (head, body) = raw.split_once("\r\n\r\n").unwrap_or_default();
    check(
        "HTTP/1.0 stream: no chunked coding, the body ends when the connection closes",
        ended
            && !head.contains("Transfer-Encoding")
            && head.contains("Connection: close")
            && body == "line 0\nline 1\nline 2\n",
    );

    // ---- 7. metrics
    println!("\n[7] /metrics (Prometheus text format)");
    let r = c.get("/metrics");
    let text = r.text();
    for l in text.lines().filter(|l| !l.starts_with('#')).take(12) {
        println!("  {l}");
    }
    check(
        "metrics expose counters, gauges and histogram",
        text.contains("# TYPE http_requests_total counter")
            && text.contains("http_request_duration_us_bucket{le=\"+Inf\"}")
            && metric_value(&text, "http_requests_total").unwrap() > 20,
    );
    check(
        "active connections gauge sees our open connections",
        metric_value(&text, "http_connections_active").unwrap() >= 2,
    );
    drop((c, p));
    let rep = srv.shutdown();
    println!(
        "  main server shut down: joined {} workers in {:?}, threads still running: {}",
        rep.workers_joined, rep.elapsed, rep.live_threads
    );
    check(
        "shutdown leaves no thread running",
        rep.live_threads == 0 && rep.workers_joined == 8,
    );

    // ---- 8. keep-alive: reuse, max requests, idle timeout
    println!("\n[8] keep-alive");
    let mut cfg = test_cfg();
    cfg.max_requests_per_conn = 100;
    cfg.idle_timeout = Duration::from_millis(250);
    let srv = Server::start(cfg, build_app(&public, ctl.clone())).unwrap();
    let mut c = Client::connect(srv.addr);
    for i in 0..50 {
        assert_eq!(c.get(&format!("/echo/{i}")).status, 200);
    }
    let text = c.get("/metrics").text();
    println!(
        "  51 requests on one connection: accepted={} requests={} reused={}",
        srv.metrics().accepted.load(SeqCst),
        srv.metrics().requests.load(SeqCst),
        srv.metrics().reused.load(SeqCst)
    );
    check(
        "keep-alive: 51 requests, exactly 1 accept",
        srv.metrics().accepted.load(SeqCst) == 1
            && metric_value(&text, "http_requests_total") == Some(50)
            && srv.metrics().reused.load(SeqCst) == 51 - 1,
    );
    let mut last_conn_hdr = String::new();
    let mut reconnects = 0;
    for i in 51..=105 {
        let r = c.get(&format!("/echo/{i}"));
        assert_eq!(r.status, 200);
        last_conn_hdr = r.header("connection").unwrap_or("").to_string();
        if last_conn_hdr == "close" {
            check(
                "server announces Connection: close on the 100th request",
                i == 99,
            );
            assert!(c.closed_by_peer());
            c = Client::connect(srv.addr);
            reconnects += 1;
        }
    }
    check(
        "max_requests_per_conn forces exactly one reconnect (2 accepts total)",
        reconnects == 1
            && srv.metrics().accepted.load(SeqCst) == 2
            && last_conn_hdr == "keep-alive",
    );
    let t = Instant::now();
    check(
        "idle keep-alive connection is closed by the server",
        c.closed_by_peer(),
    );
    let waited = t.elapsed();
    println!("  idle connection closed after {waited:?} (idle_timeout 250 ms)");
    check(
        "idle close is not earlier than the timeout and is counted",
        waited >= Duration::from_millis(150) && srv.metrics().idle_closed.load(SeqCst) >= 1,
    );
    // slow-loris: half a request and then silence
    let mut cfg = test_cfg();
    cfg.request_timeout = Duration::from_millis(300);
    let srv_slow = Server::start(cfg, build_app(&public, ctl.clone())).unwrap();
    let mut sl = Client::connect(srv_slow.addr);
    sl.send(b"GET /hello HTTP/1.1\r\nHost: t\r\nX-Slow: ");
    let r = sl.read_response(false).unwrap();
    println!("  half a request then silence -> {}", r.status);
    check(
        "slow-loris: 408 Request Timeout then close",
        r.status == 408 && sl.closed_by_peer(),
    );
    drop((c, sl));
    srv.shutdown();
    // after an error response, a client that keeps sending must not keep the worker in the lingering close
    let srv = Server::start(test_cfg(), build_app(&public, ctl.clone())).unwrap();
    let mut dripper = Client::connect(srv.addr);
    dripper.send(b"BOGUS\r\n\r\n");
    let mut got = Vec::new();
    check(
        "lingering close ends although the client keeps sending",
        drip_until(&mut dripper, &mut got, |g| {
            g.starts_with(b"HTTP/1.1 400") && srv.metrics().active.load(SeqCst) == 0
        }),
    );
    drop(dripper);
    srv.shutdown();
    // a client that never goes quiet must meet the same deadline
    let partial = b"GET /hello HTTP/1.1\r\nHost: t\r\nX-Drip: ";
    let mut dripper = Client::connect(srv_slow.addr);
    dripper.send(partial);
    let mut got = Vec::new();
    check(
        "a client that keeps dripping bytes still gets 408",
        drip_until(&mut dripper, &mut got, |g| g.starts_with(b"HTTP/1.1 408")),
    );
    drop(dripper);
    wait_until("no active connection", || {
        srv_slow.metrics().active.load(SeqCst) == 0
    });
    let mut dripper = Client::connect(srv_slow.addr);
    dripper.send(partial);
    wait_until("the dripper has a worker", || {
        srv_slow.metrics().active.load(SeqCst) == 1
    });
    let stopper = thread::spawn(move || srv_slow.shutdown());
    check(
        "shutdown returns while a client is still dripping",
        drip_until(&mut dripper, &mut Vec::new(), |_| stopper.is_finished()),
    );
    stopper.join().expect("shutdown thread");

    // ---- 9. saturation: bounded queue -> 503
    println!("\n[9] saturation: 2 workers, queue of 2, handlers blocked on a gate");
    let (ctl2, _rx2) = new_ctl();
    let mut cfg = test_cfg();
    cfg.workers = 2;
    cfg.queue_cap = 2;
    cfg.reject_linger = Duration::from_secs(5); // far longer than the pauses below, so a slow machine still passes
    let srv = Server::start(cfg, build_app(&public, ctl2.clone())).unwrap();
    let mut busy: Vec<Client> = (0..2).map(|_| Client::connect(srv.addr)).collect();
    for b in busy.iter_mut() {
        b.send(b"GET /block HTTP/1.1\r\nHost: t\r\n\r\n");
    }
    wait_until("both workers inside /block", || {
        ctl2.gate.entered.load(SeqCst) == 2
    });
    let mut queued: Vec<Client> = (0..2).map(|_| Client::connect(srv.addr)).collect();
    for q in queued.iter_mut() {
        q.send(b"GET /echo/queued HTTP/1.1\r\nHost: t\r\nConnection: close\r\n\r\n");
    }
    wait_until("two connections waiting in the queue", || {
        srv.metrics().queued.load(SeqCst) == 2
    });
    let mut shed = Client::connect(srv.addr);
    let r = shed.read_response(false).unwrap();
    println!(
        "  5th connection -> {} Retry-After={:?} body={:?}",
        r.status,
        r.header("retry-after"),
        r.text().trim()
    );
    check(
        "saturated pool sheds load: 503 + Retry-After",
        r.status == 503 && r.header("retry-after") == Some("1"),
    );
    check(
        "rejection is counted",
        srv.metrics().rejected.load(SeqCst) == 1,
    );
    let mut late = Client::connect(srv.addr);
    wait_until("the late sender is refused", || {
        srv.metrics().rejected.load(SeqCst) == 2
    });
    // The pauses only give a server that closes at once the time to do it, and its reset the time to come back.
    thread::sleep(Duration::from_millis(50));
    late.send(b"GET /hello HTTP/1.1\r\nHost: t\r\n\r\n");
    thread::sleep(Duration::from_millis(50));
    let mut raw = Vec::new();
    let clean = late.s.read_to_end(&mut raw).is_ok();
    check(
        "a request sent after the refusal does not turn the 503 into a reset",
        clean && raw.starts_with(b"HTTP/1.1 503"),
    );
    drop(late);
    ctl2.gate.open();
    for b in busy.iter_mut() {
        assert_eq!(b.read_response(false).unwrap().text(), "released");
    }
    busy.clear(); // a worker is pinned to its keep-alive connection until the client leaves: this is what frees the queue
    for q in queued.iter_mut() {
        assert_eq!(q.read_response(false).unwrap().text(), "echo:queued");
    }
    check(
        "admitted work (in workers and in the queue) all completed",
        true,
    );
    let mut again = Client::connect(srv.addr);
    check(
        "after the burst the server accepts again",
        again.get("/hello").status == 200,
    );
    drop((busy, queued, shed, again));
    srv.shutdown();

    // ---- 10. panic isolation with a single worker
    println!("\n[10] panic in a handler (1 worker)");
    let mut cfg = test_cfg();
    cfg.workers = 1;
    let srv = Server::start(cfg, build_app(&public, ctl.clone())).unwrap();
    let mut c = Client::connect(srv.addr);
    let r = c.get("/panic");
    println!("  GET /panic -> {} {:?}", r.status, r.text().trim());
    check("panicking handler returns 500", r.status == 500);
    check(
        "same connection keeps working",
        c.get("/hello").text() == "hello",
    );
    drop(c);
    let mut c2 = Client::connect(srv.addr); // can only be served by the one and only worker
    check(
        "the single worker survived and serves a new connection",
        c2.get("/echo/alive").text() == "echo:alive",
    );
    check(
        "panic counted, no worker-level panic",
        srv.metrics().handler_panics.load(SeqCst) == 1
            && srv.metrics().worker_panics.load(SeqCst) == 0,
    );
    drop(c2);
    srv.shutdown();

    // ---- 11. graceful shutdown with work in flight
    println!("\n[11] graceful shutdown");
    let srv = Server::start(test_cfg(), build_app(&public, ctl.clone())).unwrap();
    let addr = srv.addr;
    let mut idle = Client::connect(addr);
    assert_eq!(idle.get("/hello").status, 200); // now an idle keep-alive connection pinned to a worker
    let mut inflight = Client::connect(addr);
    inflight.send(b"GET /slow HTTP/1.1\r\nHost: t\r\n\r\n");
    slow_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("handler started");
    let rep = srv.shutdown(); // called while /slow is executing
    let r = inflight.read_response(false).unwrap();
    println!(
        "  in-flight request finished after shutdown began: {} {:?} connection={:?}",
        r.status,
        r.text(),
        r.header("connection")
    );
    check(
        "in-flight request completed with Connection: close",
        r.status == 200 && r.text() == "slow done" && r.header("connection") == Some("close"),
    );
    check(
        "idle keep-alive connection was closed",
        idle.closed_by_peer(),
    );
    check(
        "listener is closed: new connections are refused",
        TcpStream::connect_timeout(&addr, Duration::from_secs(1)).is_err(),
    );
    println!(
        "  shutdown took {:?}, joined {} workers, live threads {}",
        rep.elapsed, rep.workers_joined, rep.live_threads
    );
    check(
        "all server threads have exited",
        rep.live_threads == 0 && rep.workers_joined == 8,
    );

    // ---- 12. load test and comparison
    println!(
        "\n[12] load test: 32 clients x 800 requests, persistent connections, same routes, three servers"
    );
    let (clients, per) = (32, 800);
    let mut reports = Vec::new();
    let mut cfg = test_cfg();
    cfg.workers = 32;
    cfg.queue_cap = 64;
    let pool = Server::start(cfg.clone(), build_app(&public, ctl.clone())).unwrap();
    reports.push(load_test(
        "thread pool (32 workers)",
        pool.addr,
        clients,
        per,
    ));
    let pool_metrics = Client::connect(pool.addr).get("/metrics").text();
    let rep = pool.shutdown();
    check(
        "pool shut down cleanly after the load",
        rep.live_threads == 0,
    );
    cfg.mode = Mode::PerConn;
    let per_conn = Server::start(cfg, build_app(&public, ctl.clone())).unwrap();
    reports.push(load_test(
        "thread per connection",
        per_conn.addr,
        clients,
        per,
    ));
    check(
        "thread-per-connection shut down cleanly",
        per_conn.shutdown().live_threads == 0,
    );
    let tk = start_tokio(build_app(&public, ctl.clone()), test_cfg().limits);
    reports.push(load_test(
        "tokio (4 threads), same parser/router",
        tk.addr,
        clients,
        per,
    ));
    let tk_accepted = tk.app.metrics.accepted.load(SeqCst);
    tk.rt.shutdown_timeout(Duration::from_secs(2));
    println!(
        "  {:<40} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9}",
        "server", "req/s", "p50 us", "p90 us", "p99 us", "max us", "ok"
    );
    for r in &reports {
        println!(
            "  {:<40} {:>9.0} {:>9} {:>9} {:>9} {:>9} {:>6}/{}",
            r.name,
            r.rps(),
            r.hist.quantile(0.5),
            r.hist.quantile(0.9),
            r.hist.quantile(0.99),
            r.hist.quantile(1.0),
            r.ok,
            r.total
        );
    }
    println!(
        "  pool server saw: accepted={:?} requests={:?} (one connection per client, {} total requests)",
        metric_value(&pool_metrics, "http_connections_accepted_total"),
        metric_value(&pool_metrics, "http_requests_total"),
        clients * per
    );
    for r in &reports {
        check(
            &format!(
                "{}: all {} requests succeeded with correct bodies",
                r.name, r.total
            ),
            r.ok == r.total && r.total >= 10_000 && r.hist.count() == r.total,
        );
    }
    check(
        "persistent connections: one accept per client (+1 for the /metrics client)",
        metric_value(&pool_metrics, "http_connections_accepted_total") == Some(33)
            && tk_accepted == 32,
    );
    println!(
        "  (numbers are machine- and run-dependent: blocking threads are competitive at 32 connections; the event loop wins when idle connections number in the thousands)"
    );

    let _ = fs::remove_dir_all(&base);
    println!("\nALL CHECKS PASSED");
}
