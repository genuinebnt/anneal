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
        Limits { max_request_line: 2048, max_header_bytes: 8192, max_headers: 64, max_body: 64 * 1024 }
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
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
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
    let incomplete = Parsed::Incomplete { expect_continue: false };
    match find(buf, b"\r\n") {
        Some(line_end) if line_end > limits.max_request_line => return Parsed::Bad(UriTooLong),
        None if open_line_len(buf) > limits.max_request_line => return Parsed::Bad(UriTooLong),
        _ => {}
    }
    let Some(head_end) = find(buf, b"\r\n\r\n") else {
        return if buf.len() > limits.max_header_bytes { Parsed::Bad(HeaderTooLarge) } else { incomplete };
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
    if method.is_empty() || !method.bytes().all(|b| b.is_ascii_uppercase()) || !target.starts_with('/') {
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
        headers.push((name.to_ascii_lowercase(), value.trim_matches([' ', '\t']).to_string()));
    }
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let mut req = Request { method: method.to_string(), path: path.to_string(), query: query.to_string(), minor, headers, body: Vec::new() };
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
    let expects = req.header("expect").is_some_and(|v| v.eq_ignore_ascii_case("100-continue"));
    let waiting = Parsed::Incomplete { expect_continue: expects };
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
            return if open_line_len(rest) > MAX_CHUNK_LINE { Err(HttpError::BadRequest) } else { Ok(None) };
        };
        if end > MAX_CHUNK_LINE {
            return Err(HttpError::BadRequest);
        }
        let digits = rest[..end].split(|&b| b == b';').next().unwrap_or_default();
        if digits.is_empty() || digits.len() > 8 || !digits.iter().all(u8::is_ascii_hexdigit) {
            return Err(HttpError::BadRequest);
        }
        let size = digits.iter().fold(0usize, |n, &d| n * 16 + (d as char).to_digit(16).unwrap_or(0) as usize);
        pos += end + 2;
        if size == 0 {
            let trailers = pos;
            loop {
                let rest = &buf[pos..];
                let Some(end) = find(rest, b"\r\n") else {
                    return if buf.len() - trailers > limits.max_header_bytes { Err(HttpError::HeaderTooLarge) } else { Ok(None) };
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
            return if have.len() > size && have[size] != b'\r' { Err(HttpError::BadRequest) } else { Ok(None) };
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
        Response { status, headers: Vec::new(), body: Body::Empty }
    }

    /// A plain-text response.
    pub fn text(status: u16, text: impl Into<String>) -> Response {
        Response::new(status).header("Content-Type", "text/plain; charset=utf-8").bytes(text.into().into_bytes())
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
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

/// Sends `response`. Returns the number of bytes written.
pub fn write_response<W: Write>(out: &mut W, response: Response, head_only: bool, keep_alive: bool) -> io::Result<u64> {
    let mut head = format!("HTTP/1.1 {} {}\r\n", response.status, reason(response.status)).into_bytes();
    for (name, value) in &response.headers {
        write!(head, "{name}: {value}\r\n")?;
    }
    write!(head, "Connection: {}\r\n", if keep_alive { "keep-alive" } else { "close" })?;
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

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// The values a route's pattern captured from the path.
#[derive(Debug, Default)]
pub struct Params(Vec<(String, String)>);

impl Params {
    pub fn get(&self, name: &str) -> Option<&str> {
        self.0.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
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
                    Some(value) if !value.is_empty() => found.push((name.clone(), value.to_string())),
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
    pub fn add(&mut self, method: &str, pattern: &str, handler: impl Fn(&Request, &Params) -> Response + Send + Sync + 'static) {
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
        self.routes.push(Route { method: method.to_string(), pattern, handler: Box::new(handler) });
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
        App { router, steps: Vec::new() }
    }

    /// Adds a step around the router. The first one added is the outermost.
    pub fn wrap(mut self, step: impl Fn(&Request, Next<'_>) -> Response + Send + Sync + 'static) -> App {
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
        let sane = |v: &str| !v.is_empty() && v.len() <= 64 && v.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
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
use std::thread;
use std::time::{Duration, Instant};
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};
#[derive(Clone)]
pub struct Config {
    pub limits: Limits,
    /// Threads that serve connections.
    pub workers: usize,
    /// Connections that may wait for a worker.
    pub queue_cap: usize,
    /// How long a connection may wait for the first byte of its next request.
    pub idle_timeout: Duration,
    /// How long a request may take to arrive, from its first byte to its last.
    pub request_timeout: Duration,
    /// The response to this many requests closes the connection.
    pub max_requests_per_conn: usize,
    /// Where the server records what it does.
    pub metrics: Arc<Metrics>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            limits: Limits::default(),
            workers: 4,
            queue_cap: 16,
            idle_timeout: Duration::from_secs(5),
            request_timeout: Duration::from_secs(2),
            max_requests_per_conn: 100,
            metrics: Arc::new(Metrics::default()),
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
            thread::Builder::new().name(format!("worker-{i}")).spawn(move || worker(&shared, &receiver))?;
        }
        let metrics = shared.config.metrics.clone();
        thread::Builder::new().name("rejector".into()).spawn(move || reject_all(reject_receiver, &metrics))?;
        let metrics = shared.config.metrics.clone();
        thread::Builder::new().name("acceptor".into()).spawn(move || acceptor(listener, &metrics, sender, reject_sender))?;
        Ok(Server { addr })
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }
}

fn acceptor(listener: TcpListener, metrics: &Metrics, workers: SyncSender<TcpStream>, rejector: SyncSender<TcpStream>) {
    for stream in listener.incoming().flatten() {
        metrics.accepted.fetch_add(1, Ordering::SeqCst);
        // counted before the hand-over, so a worker can never take it below zero
        metrics.queued.fetch_add(1, Ordering::SeqCst);
        if let Err(error) = workers.try_send(stream) {
            metrics.queued.fetch_sub(1, Ordering::SeqCst);
            if let TrySendError::Full(stream) = error {
                metrics.rejected.fetch_add(1, Ordering::SeqCst);
                let _ = rejector.try_send(stream);
            }
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
        shared.config.metrics.queued.fetch_sub(1, Ordering::SeqCst);
        let _ = catch_unwind(AssertUnwindSafe(|| serve_connection(shared, stream)));
    }
}

/// Runs on its own thread, so that a slow rejected client never delays `accept`.
fn reject_all(receiver: Receiver<TcpStream>, metrics: &Metrics) {
    for mut stream in receiver {
        let _ = stream.set_write_timeout(Some(Duration::from_millis(200)));
        let response = Response::text(503, "503 Service Unavailable\n").header("Retry-After", "1");
        metrics.count_response(503);
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
fn wants_keep_alive(request: &Request) -> bool {
    let has = |token: &str| {
        request.headers.iter().filter(|(k, _)| k == "connection").flat_map(|(_, v)| v.split(',')).any(|t| t.trim().eq_ignore_ascii_case(token))
    };
    !has("close") && (request.minor == 1 || has("keep-alive"))
}

/// Writes a last response and closes. Counted before the write: a client that has the response sees it counted.
fn final_reply(mut stream: TcpStream, response: Response, metrics: &Metrics) {
    metrics.count_response(response.status);
    if write_response(&mut stream, response, false, false).is_ok() {
        close_politely(stream);
    }
}

fn serve_connection(shared: &Shared, mut stream: TcpStream) {
    let config = &shared.config;
    let metrics = &config.metrics;
    let _active = Gauge::raise(&metrics.active);
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_nodelay(true);
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let mut served = 0;
    let mut sent_continue = false;
    let mut idle_since = Instant::now();
    // when the first byte of the request now in `buf` arrived
    let mut started: Option<Instant> = None;
    loop {
        if !buf.is_empty() {
            match parse_request(&buf, &config.limits) {
                Parsed::Done(request, used) => {
                    buf.drain(..used);
                    served += 1;
                    if served > 1 {
                        metrics.reused.fetch_add(1, Ordering::SeqCst);
                    }
                    let began = Instant::now();
                    let response = fit_to_version(shared.app.handle(&request), request.minor);
                    metrics.durations.record(began.elapsed().as_micros() as u64);
                    let status = response.status;
                    let keep = wants_keep_alive(&request) && served < config.max_requests_per_conn;
                    if write_response(&mut stream, response, request.method == "HEAD", keep).is_err() {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(3));
                    metrics.count_response(status);
                    if !keep {
                        close_politely(stream);
                        return;
                    }
                    sent_continue = false;
                    idle_since = Instant::now();
                    started = (!buf.is_empty()).then_some(idle_since);
                    continue;
                }
                Parsed::Bad(error) => {
                    final_reply(stream, error_response(error), metrics);
                    return;
                }
                Parsed::Incomplete { expect_continue } => {
                    if expect_continue && !sent_continue {
                        sent_continue = true;
                        if stream.write_all(b"HTTP/1.1 100 Continue\r\n\r\n").is_err() {
                            return;
                        }
                    }
                }
            }
        }
        // checked on every pass: a client that keeps sending single bytes never lets a read time out
        let deadline = match started {
            Some(at) => at + config.request_timeout,
            None => idle_since + config.idle_timeout,
        };
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            if started.is_some() {
                final_reply(stream, Response::text(408, "408 Request Timeout\n"), metrics);
            }
            return;
        }
        let _ = stream.set_read_timeout(Some(left.max(Duration::from_millis(1))));
        match stream.read(&mut chunk) {
            Ok(0) => return,
            Ok(n) => {
                if buf.is_empty() {
                    started = Some(Instant::now());
                }
                buf.extend_from_slice(&chunk[..n]);
            }
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted) => {}
            Err(_) => return,
        }
    }
}
// ============================================================ static files (step 8)

use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// `Thu, 01 Jan 1970 00:00:00 GMT`: the date format HTTP headers use.
pub fn http_date(t: SystemTime) -> String {
    let secs = t.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // days since 1970-01-01 to year, month, day (Howard Hinnant's civil_from_days)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    const WEEKDAY: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    const MONTH: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    format!(
        "{}, {:02} {} {} {:02}:{:02}:{:02} GMT",
        WEEKDAY[(days + 4).rem_euclid(7) as usize],
        day,
        MONTH[(month - 1) as usize],
        year,
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}
fn percent_decode(raw: &str) -> Option<String> {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let high = (*bytes.get(i + 1)? as char).to_digit(16)?;
            let low = (*bytes.get(i + 2)? as char).to_digit(16)?;
            out.push((high * 16 + low) as u8);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

enum Range {
    Whole,
    /// Inclusive.
    Part(usize, usize),
    Unsatisfiable,
}

fn byte_range(header: &str, len: usize) -> Range {
    let Some((first, last)) = header.trim().strip_prefix("bytes=").and_then(|s| s.split_once('-')) else {
        return Range::Whole;
    };
    let number = |s: &str| -> Option<usize> {
        (!s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())).then(|| s.parse().unwrap_or(usize::MAX))
    };
    if first.is_empty() {
        return match number(last) {
            Some(n) if n > 0 && len > 0 => Range::Part(len.saturating_sub(n), len - 1),
            Some(_) => Range::Unsatisfiable,
            None => Range::Whole,
        };
    }
    let Some(start) = number(first) else {
        return Range::Whole;
    };
    let end = match number(last) {
        Some(end) if end >= start => end,
        None if last.is_empty() => usize::MAX,
        _ => return Range::Whole,
    };
    if start >= len {
        Range::Unsatisfiable
    } else {
        Range::Part(start, end.min(len - 1))
    }
}

/// Answers a request for the file `raw` (still percent-encoded) under the folder `root`.
pub fn serve_static(root: &Path, raw: &str, request: &Request) -> Response {
    let forbidden = || Response::text(403, "403 Forbidden\n");
    let missing = || Response::text(404, "404 Not Found\n");
    let Some(decoded) = percent_decode(raw) else {
        return Response::text(400, "400 Bad Request\n");
    };
    if decoded.contains(['\0', '\\']) {
        return forbidden();
    }
    let Ok(root) = fs::canonicalize(root) else {
        return missing();
    };
    let mut path = root.clone();
    for part in decoded.split('/') {
        match part {
            "" | "." => {}
            ".." => return forbidden(),
            name => path.push(name),
        }
    }
    let Ok(real) = fs::canonicalize(&path) else {
        return missing();
    };
    if !real.starts_with(&root) {
        return forbidden(); // a link that leads out of the folder
    }
    let (Ok(meta), Ok(data)) = (fs::metadata(&real), fs::read(&real)) else {
        return missing();
    };
    let modified = meta.modified().unwrap_or(UNIX_EPOCH);
    let nanos = modified.duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let etag = format!("\"{:x}-{:x}\"", data.len(), nanos);
    let last_modified = http_date(modified);
    let validators = |r: Response| r.header("ETag", etag.clone()).header("Last-Modified", last_modified.clone()).header("Accept-Ranges", "bytes");
    let unchanged = match request.header("if-none-match") {
        Some(tags) => tags.split(',').map(str::trim).any(|t| t == "*" || t.strip_prefix("W/").unwrap_or(t) == etag),
        None => request.header("if-modified-since") == Some(last_modified.as_str()),
    };
    if unchanged {
        return validators(Response::new(304));
    }
    let content_type = match real.extension().and_then(|e| e.to_str()) {
        Some("txt") => "text/plain; charset=utf-8",
        Some("html") => "text/html; charset=utf-8",
        Some("json") => "application/json",
        _ => "application/octet-stream",
    };
    let range = match request.header("range") {
        Some(header) if request.method == "GET" => byte_range(header, data.len()),
        _ => Range::Whole,
    };
    match range {
        Range::Whole => validators(Response::new(200)).header("Content-Type", content_type).bytes(data),
        Range::Part(start, end) => validators(Response::new(206))
            .header("Content-Type", content_type)
            .header("Content-Range", format!("bytes {start}-{end}/{}", data.len()))
            .bytes(data[start..=end].to_vec()),
        Range::Unsatisfiable => validators(Response::text(416, "416 Range Not Satisfiable\n")).header("Content-Range", format!("bytes */{}", data.len())),
    }
}
// ============================================================ metrics (step 9)

use std::sync::atomic::AtomicI64;

const SLOTS: usize = 16 + 8 * 60;

/// Times in microseconds, in fixed memory: 8 slots for each power of two, so a slot is never wider than 1/8 of its values.
pub struct Histogram {
    slots: Vec<AtomicU64>,
    count: AtomicU64,
    sum: AtomicU64,
    max: AtomicU64,
}

fn slot_of(v: u64) -> usize {
    if v < 16 {
        return v as usize;
    }
    let power = 63 - v.leading_zeros() as usize;
    16 + (power - 4) * 8 + ((v >> (power - 3)) & 7) as usize
}

/// The largest value that falls in `slot`.
fn slot_top(slot: usize) -> u64 {
    if slot < 16 {
        return slot as u64;
    }
    let (power, part) = ((slot - 16) / 8 + 4, ((slot - 16) % 8) as u64);
    // written so that the very last slot does not overflow
    ((1u64 << power) - 1) + ((part + 1) << (power - 3))
}

impl Default for Histogram {
    fn default() -> Self {
        Histogram { slots: (0..SLOTS).map(|_| AtomicU64::new(0)).collect(), count: AtomicU64::new(0), sum: AtomicU64::new(0), max: AtomicU64::new(0) }
    }
}

impl Histogram {
    pub fn record(&self, v: u64) {
        self.slots[slot_of(v)].fetch_add(1, Ordering::SeqCst);
        let _ = self.sum.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |s| Some(s.saturating_add(v)));
        self.max.fetch_max(v, Ordering::SeqCst);
        self.count.fetch_add(1, Ordering::SeqCst);
    }

    pub fn count(&self) -> u64 {
        self.count.load(Ordering::SeqCst)
    }

    pub fn sum(&self) -> u64 {
        self.sum.load(Ordering::SeqCst)
    }

    /// At least the value of rank `ceil(q * count)` (rank 1 at least), at most an eighth above it. 0 when empty.
    pub fn quantile(&self, q: f64) -> u64 {
        let rank = ((q * self.count() as f64).ceil() as u64).max(1);
        let max = self.max.load(Ordering::SeqCst);
        let mut seen = 0;
        for (i, slot) in self.slots.iter().enumerate() {
            seen += slot.load(Ordering::SeqCst);
            if seen >= rank {
                return slot_top(i).min(max);
            }
        }
        max
    }
}

/// Upper bounds, in microseconds, of the duration buckets in the report.
const BOUNDS: [u64; 9] = [100, 250, 500, 1_000, 2_500, 5_000, 10_000, 50_000, 100_000];

/// Everything the server counts.
#[derive(Default)]
pub struct Metrics {
    accepted: AtomicU64,
    rejected: AtomicU64,
    active: AtomicI64,
    queued: AtomicI64,
    requests: AtomicU64,
    reused: AtomicU64,
    /// By status class: index 1 is 1xx.
    classes: [AtomicU64; 6],
    durations: Durations,
}

/// A histogram that also fills the report's exact buckets.
#[derive(Default)]
struct Durations {
    histogram: Histogram,
    buckets: [AtomicU64; BOUNDS.len()],
}

impl Durations {
    fn record(&self, v: u64) {
        if let Some(i) = BOUNDS.iter().position(|&bound| v <= bound) {
            self.buckets[i].fetch_add(1, Ordering::SeqCst);
        }
        self.histogram.record(v);
    }
}

impl Metrics {
    fn count_response(&self, status: u16) {
        self.requests.fetch_add(1, Ordering::SeqCst);
        self.classes[usize::from(status / 100).min(5)].fetch_add(1, Ordering::SeqCst);
    }

    /// The report, in the Prometheus text format.
    pub fn render(&self) -> String {
        let load = |a: &AtomicU64| a.load(Ordering::SeqCst);
        let mut out = String::new();
        let mut line = |name: &str, kind: &str, value: String| {
            out.push_str(&format!("# TYPE {name} {kind}\n{name} {value}\n"));
        };
        line("http_connections_accepted_total", "counter", load(&self.accepted).to_string());
        line("http_connections_rejected_total", "counter", load(&self.rejected).to_string());
        line("http_connections_active", "gauge", self.active.load(Ordering::SeqCst).to_string());
        line("http_queue_depth", "gauge", self.queued.load(Ordering::SeqCst).to_string());
        line("http_requests_total", "counter", load(&self.requests).to_string());
        line("http_requests_reused_total", "counter", load(&self.reused).to_string());
        let h = &self.durations.histogram;
        line("http_request_duration_us_p50", "gauge", h.quantile(0.5).to_string());
        line("http_request_duration_us_p99", "gauge", h.quantile(0.99).to_string());
        out.push_str("# TYPE http_responses_total counter\n");
        for class in 1..=5 {
            out.push_str(&format!("http_responses_total{{class=\"{class}xx\"}} {}\n", load(&self.classes[class])));
        }
        out.push_str("# TYPE http_request_duration_us histogram\n");
        let mut cumulative = 0;
        for (i, bound) in BOUNDS.iter().enumerate() {
            cumulative += load(&self.durations.buckets[i]);
            out.push_str(&format!("http_request_duration_us_bucket{{le=\"{bound}\"}} {cumulative}\n"));
        }
        out.push_str(&format!("http_request_duration_us_bucket{{le=\"+Inf\"}} {}\n", h.count()));
        out.push_str(&format!("http_request_duration_us_sum {}\nhttp_request_duration_us_count {}\n", h.sum(), h.count()));
        out
    }
}

/// A handler that serves the report, for a route such as `GET /metrics`.
pub fn metrics_handler(metrics: Arc<Metrics>) -> impl Fn(&Request, &Params) -> Response + Send + Sync + 'static {
    move |_, _| Response::new(200).header("Content-Type", "text/plain; version=0.0.4").bytes(metrics.render().into_bytes())
}

/// Counts one up now and one down when dropped, however the scope is left.
struct Gauge<'a>(&'a AtomicI64);

impl<'a> Gauge<'a> {
    fn raise(value: &'a AtomicI64) -> Self {
        value.fetch_add(1, Ordering::SeqCst);
        Gauge(value)
    }
}

impl Drop for Gauge<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}
