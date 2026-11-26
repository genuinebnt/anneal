// Capstone, step 7 of 10: keep connections open, with time limits.
//
// Everything outside the `todo!()`s is the reference code of the earlier steps. It is a STAND-IN until the app can
// carry your own code from step to step: replace it with yours if you like. The public names must stay.
//
// SPOILER: this code solves the earlier steps, and much of the preparation before them.

// ============================================================ requests (steps 1 and 2)

/// Size limits a request must stay within.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Bytes in the request line, without its CRLF.
    pub max_request_line: usize,
    /// Bytes in the whole head: request line, headers and the blank line.
    pub max_header_bytes: usize,
    /// Number of header lines.
    pub max_headers: usize,
    /// Bytes in the (decoded) body.
    pub max_body: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_request_line: 2048,
            max_header_bytes: 8192,
            max_headers: 64,
            max_body: 64 * 1024,
        }
    }
}

/// Why a request can never be served.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpError {
    BadRequest,
    UriTooLong,
    HeaderTooLarge,
    BodyTooLarge,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Request {
    pub method: String,
    /// The target up to the first `?`.
    pub path: String,
    /// What follows the first `?`, or empty.
    pub query: String,
    /// 0 for HTTP/1.0, 1 for HTTP/1.1.
    pub minor: u8,
    /// In arrival order, names in lower case.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// What the bytes received so far amount to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Parsed {
    /// Not a whole request yet. `expect_continue`: the head is complete, it asked for `100-continue`, and the body is not all here.
    Incomplete { expect_continue: bool },
    /// One whole request, and how many bytes of the buffer it used.
    Done(Request, usize),
    /// No further bytes can make this a request the server accepts.
    Bad(HttpError),
}
impl HttpError {
    /// The response status for this error.
    pub fn status(self) -> u16 {
        match self {
            HttpError::BadRequest => 400,
            HttpError::UriTooLong => 414,
            HttpError::HeaderTooLarge => 431,
            HttpError::BodyTooLarge => 413,
        }
    }
}

impl Request {
    /// The first header with this name, compared without regard to case.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}
/// A chunk-size line (size plus any `;extension`), without its CRLF.
const MAX_CHUNK_LINE: usize = 64;

/// Length of a line that has not ended yet; a trailing `\r` may be the first half of its CRLF.
fn open_line_len(tail: &[u8]) -> usize {
    tail.len() - usize::from(tail.last() == Some(&b'\r'))
}

/// Reads one request from the front of `buf`. Pure: call it again whenever more bytes have arrived.
pub fn parse_request(buf: &[u8], limits: &Limits) -> Parsed {
    use HttpError::*;
    let incomplete = Parsed::Incomplete {
        expect_continue: false,
    };
    match find(buf, b"\r\n") {
        Some(line_end) if line_end > limits.max_request_line => return Parsed::Bad(UriTooLong),
        None if open_line_len(buf) > limits.max_request_line => return Parsed::Bad(UriTooLong),
        _ => {}
    }
    let Some(head_end) = find(buf, b"\r\n\r\n") else {
        return if buf.len() > limits.max_header_bytes {
            Parsed::Bad(HeaderTooLarge)
        } else {
            incomplete
        };
    };
    let head_len = head_end + 4;
    if head_len > limits.max_header_bytes {
        return Parsed::Bad(HeaderTooLarge);
    }
    let Ok(head) = std::str::from_utf8(&buf[..head_end]) else {
        return Parsed::Bad(BadRequest);
    };
    let mut lines = head.split("\r\n");
    let parts: Vec<&str> = lines.next().unwrap_or("").split(' ').collect();
    let [method, target, version] = parts[..] else {
        return Parsed::Bad(BadRequest);
    };
    if method.is_empty()
        || !method.bytes().all(|b| b.is_ascii_uppercase())
        || !target.starts_with('/')
    {
        return Parsed::Bad(BadRequest);
    }
    let minor = match version {
        "HTTP/1.1" => 1,
        "HTTP/1.0" => 0,
        _ => return Parsed::Bad(BadRequest),
    };
    let mut headers: Vec<(String, String)> = Vec::new();
    for line in lines {
        if headers.len() >= limits.max_headers {
            return Parsed::Bad(HeaderTooLarge);
        }
        if line.starts_with([' ', '\t']) {
            return Parsed::Bad(BadRequest);
        }
        let Some((name, value)) = line.split_once(':') else {
            return Parsed::Bad(BadRequest);
        };
        if name.is_empty() || name.bytes().any(|b| b <= b' ' || b == 0x7f) {
            return Parsed::Bad(BadRequest);
        }
        headers.push((
            name.to_ascii_lowercase(),
            value.trim_matches([' ', '\t']).to_string(),
        ));
    }
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let mut req = Request {
        method: method.to_string(),
        path: path.to_string(),
        query: query.to_string(),
        minor,
        headers,
        body: Vec::new(),
    };
    if minor == 1 && req.header("host").is_none() {
        return Parsed::Bad(BadRequest);
    }
    let mut length: Option<usize> = None;
    let mut encodings = 0;
    let mut chunked = false;
    for (name, value) in &req.headers {
        if name == "content-length" {
            if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
                return Parsed::Bad(BadRequest);
            }
            // all digits, so the only way to fail is a number too large to hold
            let n = value.parse::<usize>().unwrap_or(usize::MAX);
            if length.is_some_and(|earlier| earlier != n) {
                return Parsed::Bad(BadRequest);
            }
            length = Some(n);
        } else if name == "transfer-encoding" {
            encodings += 1;
            chunked = value.eq_ignore_ascii_case("chunked");
        }
    }
    if encodings > 0 && (encodings > 1 || !chunked || length.is_some() || minor == 0) {
        return Parsed::Bad(BadRequest);
    }
    let expects = req
        .header("expect")
        .is_some_and(|v| v.eq_ignore_ascii_case("100-continue"));
    let waiting = Parsed::Incomplete {
        expect_continue: expects,
    };
    if chunked {
        return match decode_chunks(&buf[head_len..], limits) {
            Err(e) => Parsed::Bad(e),
            Ok(None) => waiting,
            Ok(Some((body, used))) => {
                req.body = body;
                Parsed::Done(req, head_len + used)
            }
        };
    }
    let n = length.unwrap_or(0);
    if n > limits.max_body {
        return Parsed::Bad(BodyTooLarge);
    }
    if buf.len() < head_len + n {
        return waiting;
    }
    req.body = buf[head_len..head_len + n].to_vec();
    Parsed::Done(req, head_len + n)
}

/// A decoded body and the encoded bytes it used.
type Decoded = Option<(Vec<u8>, usize)>;

/// Decodes a chunked body from the front of `buf`. `Ok(None)`: more bytes are needed.
fn decode_chunks(buf: &[u8], limits: &Limits) -> Result<Decoded, HttpError> {
    let mut body = Vec::new();
    let mut pos = 0;
    loop {
        let rest = &buf[pos..];
        let Some(end) = find(rest, b"\r\n") else {
            return if open_line_len(rest) > MAX_CHUNK_LINE {
                Err(HttpError::BadRequest)
            } else {
                Ok(None)
            };
        };
        if end > MAX_CHUNK_LINE {
            return Err(HttpError::BadRequest);
        }
        let digits = rest[..end].split(|&b| b == b';').next().unwrap_or_default();
        if digits.is_empty() || digits.len() > 8 || !digits.iter().all(u8::is_ascii_hexdigit) {
            return Err(HttpError::BadRequest);
        }
        let size = digits.iter().fold(0usize, |n, &d| {
            n * 16 + (d as char).to_digit(16).unwrap_or(0) as usize
        });
        pos += end + 2;
        if size == 0 {
            let trailers = pos;
            loop {
                let rest = &buf[pos..];
                let Some(end) = find(rest, b"\r\n") else {
                    return if buf.len() - trailers > limits.max_header_bytes {
                        Err(HttpError::HeaderTooLarge)
                    } else {
                        Ok(None)
                    };
                };
                pos += end + 2;
                if pos - trailers > limits.max_header_bytes {
                    return Err(HttpError::HeaderTooLarge);
                }
                if end == 0 {
                    return Ok(Some((body, pos)));
                }
            }
        }
        if size > limits.max_body - body.len() {
            return Err(HttpError::BodyTooLarge);
        }
        let have = &buf[pos..];
        if have.len() < size + 2 {
            // the CR may already be here and wrong
            return if have.len() > size && have[size] != b'\r' {
                Err(HttpError::BadRequest)
            } else {
                Ok(None)
            };
        }
        if &have[size..size + 2] != b"\r\n" {
            return Err(HttpError::BadRequest);
        }
        body.extend_from_slice(&have[..size]);
        pos += size + 2;
    }
}
// ============================================================ responses (step 3)

use std::io::{self, Write};

pub enum Body {
    Empty,
    Bytes(Vec<u8>),
    /// Produced piece by piece while it is being sent.
    Stream(Box<dyn Iterator<Item = Vec<u8>> + Send>),
}

pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Body,
}

/// The reason phrase for the status codes this server sends.
pub fn reason(status: u16) -> &'static str {
    match status {
        100 => "Continue",
        200 => "OK",
        204 => "No Content",
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
impl Response {
    /// A response with no headers and an empty body.
    pub fn new(status: u16) -> Response {
        Response {
            status,
            headers: Vec::new(),
            body: Body::Empty,
        }
    }

    /// A plain-text response.
    pub fn text(status: u16, text: impl Into<String>) -> Response {
        Response::new(status)
            .header("Content-Type", "text/plain; charset=utf-8")
            .bytes(text.into().into_bytes())
    }

    pub fn header(mut self, name: &str, value: impl Into<String>) -> Response {
        self.headers.push((name.to_string(), value.into()));
        self
    }

    pub fn bytes(mut self, body: Vec<u8>) -> Response {
        self.body = Body::Bytes(body);
        self
    }

    pub fn stream(mut self, pieces: impl Iterator<Item = Vec<u8>> + Send + 'static) -> Response {
        self.body = Body::Stream(Box::new(pieces));
        self
    }

    /// The first header with this name, compared without regard to case.
    pub fn header_value(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// Sends `response`. Returns the number of bytes written.
pub fn write_response<W: Write>(
    out: &mut W,
    response: Response,
    head_only: bool,
    keep_alive: bool,
) -> io::Result<u64> {
    let mut head = format!(
        "HTTP/1.1 {} {}\r\n",
        response.status,
        reason(response.status)
    )
    .into_bytes();
    for (name, value) in &response.headers {
        write!(head, "{name}: {value}\r\n")?;
    }
    write!(
        head,
        "Connection: {}\r\n",
        if keep_alive { "keep-alive" } else { "close" }
    )?;
    let bodyless = response.status / 100 == 1 || response.status == 204 || response.status == 304;
    let bytes = match response.body {
        Body::Stream(pieces) if !bodyless => {
            head.extend_from_slice(b"Transfer-Encoding: chunked\r\n\r\n");
            out.write_all(&head)?;
            let mut total = head.len() as u64;
            if !head_only {
                for piece in pieces {
                    if piece.is_empty() {
                        continue; // an empty chunk would end the body
                    }
                    let mut frame = format!("{:x}\r\n", piece.len()).into_bytes();
                    frame.extend_from_slice(&piece);
                    frame.extend_from_slice(b"\r\n");
                    out.write_all(&frame)?;
                    out.flush()?;
                    total += frame.len() as u64;
                }
                out.write_all(b"0\r\n\r\n")?;
                total += 5;
            }
            out.flush()?;
            return Ok(total);
        }
        Body::Bytes(bytes) => bytes,
        _ => Vec::new(),
    };
    if !bodyless {
        write!(head, "Content-Length: {}\r\n", bytes.len())?;
    }
    head.extend_from_slice(b"\r\n");
    if !head_only && !bodyless {
        head.extend_from_slice(&bytes);
    }
    out.write_all(&head)?;
    out.flush()?;
    Ok(head.len() as u64)
}

/// The response for a request that could not be accepted.
pub fn error_response(error: HttpError) -> Response {
    let status = error.status();
    Response::text(status, format!("{status} {}\n", reason(status)))
}
// ============================================================ routes and shared steps (step 4)

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// The values a route's pattern captured from the path.
#[derive(Debug, Default)]
pub struct Params(Vec<(String, String)>);

impl Params {
    pub fn get(&self, name: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
}

type Handler = Box<dyn Fn(&Request, &Params) -> Response + Send + Sync>;

enum Segment {
    Literal(String),
    Param(String),
    Rest(String),
}

struct Route {
    method: String,
    pattern: Vec<Segment>,
    handler: Handler,
}

impl Route {
    fn matches(&self, path: &[&str]) -> Option<Params> {
        let mut found = Vec::new();
        for (i, segment) in self.pattern.iter().enumerate() {
            match segment {
                Segment::Literal(text) => {
                    if path.get(i) != Some(&text.as_str()) {
                        return None;
                    }
                }
                Segment::Param(name) => match path.get(i) {
                    Some(value) if !value.is_empty() => {
                        found.push((name.clone(), value.to_string()))
                    }
                    _ => return None,
                },
                Segment::Rest(name) => {
                    found.push((name.clone(), path.get(i..)?.join("/")));
                    return Some(Params(found));
                }
            }
        }
        (path.len() == self.pattern.len()).then_some(Params(found))
    }
}

#[derive(Default)]
pub struct Router {
    routes: Vec<Route>,
}

/// The rest of a chain of steps: call it to go on.
pub type Next<'a> = &'a dyn Fn(&Request) -> Response;

type Step = Box<dyn Fn(&Request, Next<'_>) -> Response + Send + Sync>;

/// A router wrapped in shared steps.
pub struct App {
    router: Router,
    steps: Vec<Step>,
}
impl Router {
    pub fn new() -> Router {
        Router::default()
    }

    /// `pattern`: segments separated by `/`; `:name` matches one non-empty segment, `*name` (last) the rest of the path.
    pub fn add(
        &mut self,
        method: &str,
        pattern: &str,
        handler: impl Fn(&Request, &Params) -> Response + Send + Sync + 'static,
    ) {
        let pattern = pattern
            .split('/')
            .skip(1)
            .map(|s| {
                if let Some(name) = s.strip_prefix(':') {
                    Segment::Param(name.to_string())
                } else if let Some(name) = s.strip_prefix('*') {
                    Segment::Rest(name.to_string())
                } else {
                    Segment::Literal(s.to_string())
                }
            })
            .collect();
        self.routes.push(Route {
            method: method.to_string(),
            pattern,
            handler: Box::new(handler),
        });
    }

    pub fn dispatch(&self, request: &Request) -> Response {
        let path: Vec<&str> = request.path.split('/').skip(1).collect();
        let mut allowed: Vec<&str> = Vec::new();
        let mut head_fallback = None;
        for route in &self.routes {
            let Some(params) = route.matches(&path) else {
                continue;
            };
            if route.method == request.method {
                return (route.handler)(request, &params);
            }
            if request.method == "HEAD" && route.method == "GET" && head_fallback.is_none() {
                head_fallback = Some((route, params));
            }
            if !allowed.contains(&route.method.as_str()) {
                allowed.push(&route.method);
            }
        }
        if let Some((route, params)) = head_fallback {
            (route.handler)(request, &params)
        } else if allowed.is_empty() {
            Response::text(404, "404 Not Found\n")
        } else {
            Response::text(405, "405 Method Not Allowed\n").header("Allow", allowed.join(", "))
        }
    }
}

fn run_steps(steps: &[Step], request: &Request, router: &Router) -> Response {
    match steps.split_first() {
        None => router.dispatch(request),
        Some((step, rest)) => step(request, &|r: &Request| run_steps(rest, r, router)),
    }
}

impl App {
    pub fn new(router: Router) -> App {
        App {
            router,
            steps: Vec::new(),
        }
    }

    /// Adds a step around the router. The first one added is the outermost.
    pub fn wrap(
        mut self,
        step: impl Fn(&Request, Next<'_>) -> Response + Send + Sync + 'static,
    ) -> App {
        self.steps.push(Box::new(step));
        self
    }

    pub fn handle(&self, request: &Request) -> Response {
        run_steps(&self.steps, request, &self.router)
    }
}

/// A step that turns a panic in anything inside it into a 500.
pub fn catch_panics() -> impl Fn(&Request, Next<'_>) -> Response + Send + Sync + 'static {
    |request, next| match catch_unwind(AssertUnwindSafe(|| next(request))) {
        Ok(response) => response,
        Err(_) => Response::text(500, "500 Internal Server Error\n"),
    }
}

/// A step that gives every response an `X-Request-Id`: the client's, if it is sane, or a new one.
pub fn request_ids() -> impl Fn(&Request, Next<'_>) -> Response + Send + Sync + 'static {
    let counter = Arc::new(AtomicU64::new(1));
    move |request, next| {
        let sane = |v: &str| {
            !v.is_empty()
                && v.len() <= 64
                && v.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        };
        let id = match request.header("x-request-id") {
            Some(v) if sane(v) => v.to_string(),
            _ => format!("req-{}", counter.fetch_add(1, Ordering::Relaxed)),
        };
        next(request).header("X-Request-Id", id)
    }
}
// ============================================================ the server (steps 5 to 7, 9 and 10)

use std::io::{ErrorKind, Read};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};
#[derive(Clone)]
pub struct Config {
    pub limits: Limits,
    /// Threads that serve connections.
    pub workers: usize,
    /// Connections that may wait for a worker.
    pub queue_cap: usize,
    /// How long a connection may wait for the first byte of its next request.
    pub idle_timeout: std::time::Duration,
    /// How long a request may take to arrive, from its first byte to its last.
    pub request_timeout: std::time::Duration,
    /// The response to this many requests closes the connection.
    pub max_requests_per_conn: usize,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            limits: Limits::default(),
            workers: 4,
            queue_cap: 16,
            idle_timeout: std::time::Duration::from_secs(5),
            request_timeout: std::time::Duration::from_secs(2),
            max_requests_per_conn: 100,
        }
    }
}
struct Shared {
    app: App,
    config: Config,
}
pub struct Server {
    addr: SocketAddr,
}

impl Server {
    /// Listens on a free port of 127.0.0.1.
    pub fn start(config: Config, app: App) -> io::Result<Server> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;
        let workers = config.workers.max(1);
        let (sender, receiver) = sync_channel::<TcpStream>(config.queue_cap);
        let (reject_sender, reject_receiver) = sync_channel::<TcpStream>(64);
        let shared = Arc::new(Shared { app, config });
        let receiver = Arc::new(Mutex::new(receiver));
        for i in 0..workers {
            let (shared, receiver) = (shared.clone(), receiver.clone());
            thread::Builder::new()
                .name(format!("worker-{i}"))
                .spawn(move || worker(&shared, &receiver))?;
        }
        thread::Builder::new()
            .name("rejector".into())
            .spawn(move || reject_all(reject_receiver))?;
        thread::Builder::new()
            .name("acceptor".into())
            .spawn(move || acceptor(listener, sender, reject_sender))?;
        Ok(Server { addr })
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }
}

fn acceptor(
    listener: TcpListener,
    workers: SyncSender<TcpStream>,
    rejector: SyncSender<TcpStream>,
) {
    for stream in listener.incoming().flatten() {
        if let Err(TrySendError::Full(stream)) = workers.try_send(stream) {
            // if even the rejector is swamped the connection is simply dropped
            let _ = rejector.try_send(stream);
        }
    }
}

fn worker(shared: &Shared, receiver: &Mutex<Receiver<TcpStream>>) {
    loop {
        // the guard is dropped at the end of this statement, before the connection is served
        let job = receiver.lock().unwrap_or_else(|e| e.into_inner()).recv();
        let Ok(stream) = job else {
            return;
        };
        let _ = catch_unwind(AssertUnwindSafe(|| serve_connection(shared, stream)));
    }
}

/// Runs on its own thread, so that a slow rejected client never delays `accept`.
fn reject_all(receiver: Receiver<TcpStream>) {
    for mut stream in receiver {
        let _ = stream.set_write_timeout(Some(Duration::from_millis(200)));
        let response = Response::text(503, "503 Service Unavailable\n").header("Retry-After", "1");
        if write_response(&mut stream, response, false, false).is_ok() {
            close_politely(stream);
        }
    }
}
/// Sends our end-of-stream, then reads away what the client is still sending. Closing a socket with unread input
/// resets the connection, and the reset can destroy a response the client has not read yet. Bounded in time and size.
fn close_politely(mut stream: TcpStream) {
    let _ = stream.shutdown(Shutdown::Write);
    let deadline = Instant::now() + Duration::from_millis(500);
    let _ = stream.set_read_timeout(Some(Duration::from_millis(100)));
    let mut sink = [0u8; 4096];
    let mut total = 0;
    while Instant::now() < deadline && total < 1024 * 1024 {
        match stream.read(&mut sink) {
            Ok(0) | Err(_) => break,
            Ok(n) => total += n,
        }
    }
}

/// An HTTP/1.0 client cannot read a chunked body.
fn fit_to_version(mut response: Response, minor: u8) -> Response {
    if minor == 0 && matches!(response.body, Body::Stream(_)) {
        let Body::Stream(pieces) = std::mem::replace(&mut response.body, Body::Empty) else {
            return response;
        };
        response.body = Body::Bytes(pieces.flatten().collect());
    }
    response
}
/// One request, one response, then close.
fn serve_connection(shared: &Shared, mut stream: TcpStream) {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match parse_request(&buf, &shared.config.limits) {
            Parsed::Done(request, _) => {
                let response = fit_to_version(shared.app.handle(&request), request.minor);
                if write_response(&mut stream, response, request.method == "HEAD", false).is_ok() {
                    close_politely(stream);
                }
                return;
            }
            Parsed::Bad(error) => {
                if write_response(&mut stream, error_response(error), false, false).is_ok() {
                    close_politely(stream);
                }
                return;
            }
            Parsed::Incomplete { .. } => {}
        }
        match stream.read(&mut chunk) {
            Ok(0) => return,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(_) => return,
        }
    }
}
