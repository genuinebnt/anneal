use solution::*;

#[allow(dead_code)]
fn limits() -> Limits {
    Limits::default()
}

#[allow(dead_code)]
fn parse(bytes: &[u8]) -> Parsed {
    parse_request(bytes, &Limits::default())
}

/// The request in `bytes`, which must be exactly one whole request.
#[allow(dead_code)]
fn whole(bytes: &[u8]) -> Request {
    match parse(bytes) {
        Parsed::Done(request, used) => {
            assert_eq!(used, bytes.len(), "bytes used by {:?}", String::from_utf8_lossy(bytes));
            request
        }
        other => panic!("expected a whole request from {:?}, got {other:?}", String::from_utf8_lossy(bytes)),
    }
}

#[allow(dead_code)]
fn bad(status: u16, bytes: &[u8]) {
    let got = match parse(bytes) {
        Parsed::Bad(e) => Some(e.status()),
        _ => None,
    };
    check!(format!("{:?}", String::from_utf8_lossy(&bytes[..bytes.len().min(200)])), got, Some(status));
}

/// A request built by hand, for calling handlers directly.
#[allow(dead_code)]
fn request(method: &str, path: &str, headers: &[(&str, &str)]) -> Request {
    Request {
        method: method.to_string(),
        path: path.to_string(),
        query: String::new(),
        minor: 1,
        headers: headers.iter().map(|(k, v)| (k.to_ascii_lowercase(), v.to_string())).collect(),
        body: Vec::new(),
    }
}

/// A response as a client reads it off the wire.
#[allow(dead_code)]
#[derive(Debug, Clone)]
struct Wire {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

#[allow(dead_code)]
impl Wire {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
    fn count(&self, name: &str) -> usize {
        self.headers.iter().filter(|(k, _)| k.eq_ignore_ascii_case(name)).count()
    }
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

/// Reads responses from any byte source, keeping what belongs to the next one.
#[allow(dead_code)]
struct Reader<R: std::io::Read> {
    source: R,
    buf: Vec<u8>,
}

#[allow(dead_code)]
impl<R: std::io::Read> Reader<R> {
    fn new(source: R) -> Self {
        Reader { source, buf: Vec::new() }
    }

    fn fill(&mut self) -> Result<(), String> {
        let mut chunk = [0u8; 8192];
        match self.source.read(&mut chunk) {
            Ok(0) => Err(format!("the stream ended; unread bytes: {:?}", String::from_utf8_lossy(&self.buf))),
            Ok(n) => {
                self.buf.extend_from_slice(&chunk[..n]);
                Ok(())
            }
            Err(e) => Err(format!("read failed: {e}; unread bytes: {:?}", String::from_utf8_lossy(&self.buf))),
        }
    }

    fn until(&mut self, mark: &[u8]) -> Result<Vec<u8>, String> {
        loop {
            if let Some(i) = self.buf.windows(mark.len()).position(|w| w == mark) {
                let piece = self.buf[..i].to_vec();
                self.buf.drain(..i + mark.len());
                return Ok(piece);
            }
            self.fill()?;
        }
    }

    fn take(&mut self, n: usize) -> Result<Vec<u8>, String> {
        while self.buf.len() < n {
            self.fill()?;
        }
        Ok(self.buf.drain(..n).collect())
    }

    /// The next response. `head_only`: it answers a HEAD request, so no body follows.
    fn next(&mut self, head_only: bool) -> Result<Wire, String> {
        let head = String::from_utf8(self.until(b"\r\n\r\n")?).map_err(|e| e.to_string())?;
        let mut lines = head.split("\r\n");
        let first = lines.next().unwrap_or("");
        if !first.starts_with("HTTP/1.1 ") {
            return Err(format!("bad status line {first:?}"));
        }
        let status: u16 = first.split(' ').nth(1).and_then(|s| s.parse().ok()).ok_or_else(|| format!("bad status line {first:?}"))?;
        let mut headers = Vec::new();
        for line in lines {
            let (k, v) = line.split_once(':').ok_or_else(|| format!("bad header line {line:?}"))?;
            headers.push((k.trim().to_string(), v.trim().to_string()));
        }
        let mut wire = Wire { status, headers, body: Vec::new() };
        if head_only || status < 200 || status == 204 || status == 304 {
            return Ok(wire);
        }
        if wire.header("transfer-encoding").is_some_and(|v| v.eq_ignore_ascii_case("chunked")) {
            loop {
                let line = self.until(b"\r\n")?;
                let size = usize::from_str_radix(&String::from_utf8_lossy(&line), 16).map_err(|e| format!("bad chunk size: {e}"))?;
                if size == 0 {
                    if !self.until(b"\r\n")?.is_empty() {
                        return Err("unexpected trailer".into());
                    }
                    return Ok(wire);
                }
                wire.body.extend(self.take(size)?);
                if self.take(2)? != b"\r\n" {
                    return Err("chunk not followed by CRLF".into());
                }
            }
        }
        let length = wire.header("content-length").ok_or("a response with a body needs Content-Length or chunked encoding")?;
        let n: usize = length.parse().map_err(|_| format!("bad Content-Length {length:?}"))?;
        wire.body = self.take(n)?;
        Ok(wire)
    }

    /// True when nothing is left over and the other side has closed cleanly.
    fn at_end(&mut self) -> bool {
        if !self.buf.is_empty() {
            return false;
        }
        let mut one = [0u8; 1];
        matches!(self.source.read(&mut one), Ok(0))
    }
}

/// The single response in `bytes`, which must contain nothing else.
#[allow(dead_code)]
fn decode(bytes: &[u8], head_only: bool) -> Wire {
    let mut reader = Reader::new(bytes);
    let wire = reader.next(head_only).unwrap_or_else(|e| panic!("{e}"));
    assert!(reader.at_end(), "bytes left after the response: {:?}", String::from_utf8_lossy(&reader.buf));
    wire
}

/// How long a client waits before a test gives up. Only a broken server gets near it.
#[allow(dead_code)]
const PATIENCE: std::time::Duration = std::time::Duration::from_secs(4);

#[allow(dead_code)]
struct Client {
    reader: Reader<std::net::TcpStream>,
}

#[allow(dead_code)]
impl Client {
    fn connect(addr: std::net::SocketAddr) -> Client {
        let s = std::net::TcpStream::connect_timeout(&addr, PATIENCE).expect("connect");
        s.set_read_timeout(Some(PATIENCE)).unwrap();
        s.set_write_timeout(Some(PATIENCE)).unwrap();
        s.set_nodelay(true).unwrap();
        Client { reader: Reader::new(s) }
    }
    fn send(&mut self, bytes: &[u8]) {
        use std::io::Write as _;
        self.reader.source.write_all(bytes).expect("client write");
    }
    fn try_send(&mut self, bytes: &[u8]) -> bool {
        use std::io::Write as _;
        self.reader.source.write_all(bytes).is_ok()
    }
    fn response(&mut self) -> Wire {
        self.reader.next(false).unwrap_or_else(|e| panic!("{e}"))
    }
    fn head_response(&mut self) -> Wire {
        self.reader.next(true).unwrap_or_else(|e| panic!("{e}"))
    }
    /// The server closed the connection cleanly, with nothing more to say.
    fn closed(&mut self) -> bool {
        self.reader.at_end()
    }
    /// Stops sending, as a client does when it goes away.
    fn finish_sending(&mut self) {
        let _ = self.reader.source.shutdown(std::net::Shutdown::Write);
    }
    /// Every byte until the server closes. A reset or a timeout is a failure.
    fn everything(mut self) -> Vec<u8> {
        use std::io::Read as _;
        let mut out = std::mem::take(&mut self.reader.buf);
        if let Err(e) = self.reader.source.read_to_end(&mut out) {
            panic!("the connection did not end cleanly: {e}; read so far: {:?}", String::from_utf8_lossy(&out));
        }
        out
    }
}

/// `GET path` on a new connection that asks to be closed; the reply must be followed by a clean end of stream.
#[allow(dead_code)]
fn fetch(addr: std::net::SocketAddr, path: &str) -> Wire {
    let mut c = Client::connect(addr);
    c.send(format!("GET {path} HTTP/1.1\r\nHost: t\r\nConnection: close\r\n\r\n").as_bytes());
    decode(&c.everything(), false)
}

/// One exchange on a new connection with these exact bytes.
#[allow(dead_code)]
fn exchange(addr: std::net::SocketAddr, bytes: &[u8]) -> Wire {
    let mut c = Client::connect(addr);
    c.send(bytes);
    decode(&c.everything(), false)
}

/// Holds handlers until the test opens it, and tells the test how many have arrived.
#[allow(dead_code)]
#[derive(Clone, Default)]
struct Gate(std::sync::Arc<(std::sync::Mutex<(bool, usize)>, std::sync::Condvar)>);

#[allow(dead_code)]
impl Gate {
    fn pass(&self) {
        let (lock, cv) = &*self.0;
        let mut state = lock.lock().unwrap();
        state.1 += 1;
        cv.notify_all();
        let _ = cv.wait_timeout_while(state, PATIENCE, |s| !s.0).unwrap();
    }
    fn open(&self) {
        let (lock, cv) = &*self.0;
        lock.lock().unwrap().0 = true;
        cv.notify_all();
    }
    fn wait_for(&self, n: usize) {
        let (lock, cv) = &*self.0;
        let state = lock.lock().unwrap();
        let (state, _) = cv.wait_timeout_while(state, PATIENCE, |s| s.1 < n).unwrap();
        assert!(state.1 >= n, "only {} of {n} requests reached the handler", state.1);
    }
}

use std::time::{Duration, Instant};

fn app() -> App {
    let mut r = Router::new();
    r.add("GET", "/echo/:word", |_, p| Response::text(200, p.get("word").unwrap_or("").to_string()));
    r.add("POST", "/echo", |req, _| Response::new(200).bytes(req.body.clone()));
    r.add("GET", "/count", |_, _| Response::new(200).stream((0..3).map(|i| format!("{i};").into_bytes())));
    App::new(r)
}

fn start(config: Config) -> Server {
    Server::start(config, app()).expect("start")
}

fn get(path: &str) -> Vec<u8> {
    format!("GET {path} HTTP/1.1\r\nHost: t\r\n\r\n").into_bytes()
}

/// Sends one byte every 25 ms, keeping the connection busy, until a response starts to arrive.
fn drip_until_answered(addr: std::net::SocketAddr, first: &[u8], then: u8) -> (Wire, usize) {
    let mut c = Client::connect(addr);
    c.send(first);
    c.reader.source.set_read_timeout(Some(Duration::from_millis(25))).unwrap();
    let mut sent = 0;
    while c.reader.fill().is_err() {
        assert!(sent < 120, "no reply although {sent} single bytes were sent, 25 ms apart");
        let _ = c.try_send(&[then]);
        sent += 1;
    }
    c.reader.source.set_read_timeout(Some(PATIENCE)).unwrap();
    (c.response(), sent)
}

#[test]
fn a_client_dripping_header_bytes_gets_408_while_still_sending() {
    let server = start(Config { request_timeout: Duration::from_millis(300), ..Config::default() });
    let (w, _) = drip_until_answered(server.addr(), b"GET / HTTP/1.1\r\nX: ", b'a');
    check!("dripped header", w.status, 408);
}

#[test]
fn a_client_dripping_body_bytes_gets_408_too() {
    let server = start(Config { request_timeout: Duration::from_millis(300), ..Config::default() });
    let (w, _) = drip_until_answered(server.addr(), b"POST /echo HTTP/1.1\r\nHost: t\r\nContent-Length: 1000\r\n\r\n", b'b');
    check!("dripped body", w.status, 408);
}

#[test]
fn the_request_clock_starts_with_the_request_not_the_connection() {
    // idle for longer than the request timeout, then send a request in two parts
    let server = start(Config { request_timeout: Duration::from_millis(500), idle_timeout: Duration::from_secs(5), ..Config::default() });
    let mut c = Client::connect(server.addr());
    std::thread::sleep(Duration::from_millis(650));
    c.send(b"GET /echo/late HTTP/1.1\r\n");
    std::thread::sleep(Duration::from_millis(50));
    c.send(b"Host: t\r\n\r\n");
    check!("not timed out", c.response().text(), "late".to_string());
}

#[test]
fn a_request_may_take_longer_than_the_idle_timeout() {
    let server = start(Config { idle_timeout: Duration::from_millis(200), request_timeout: Duration::from_secs(3), ..Config::default() });
    let mut c = Client::connect(server.addr());
    c.send(b"GET /echo/slow HTTP/1.1\r\n");
    std::thread::sleep(Duration::from_millis(450));
    c.send(b"Host: t\r\n\r\n");
    check!("still served", c.response().text(), "slow".to_string());
}

#[test]
fn a_connection_that_never_sends_is_closed_after_the_idle_timeout() {
    let server = start(Config { idle_timeout: Duration::from_millis(250), workers: 1, ..Config::default() });
    let mut silent = Client::connect(server.addr());
    // the only worker is with the silent client; this one is served once it has been dropped
    let began = Instant::now();
    check!("the next client gets its turn", fetch(server.addr(), "/echo/next").status, 200);
    assert!(began.elapsed() < Duration::from_secs(3));
    check!("silent one closed without a response", silent.closed(), true);
}

#[test]
fn the_last_allowed_request_says_close() {
    let server = start(Config { max_requests_per_conn: 3, ..Config::default() });
    let mut c = Client::connect(server.addr());
    c.send(&[get("/echo/1"), get("/echo/2"), get("/echo/3"), get("/echo/4")].concat());
    let got: Vec<(String, String)> = (0..3)
        .map(|_| {
            let w = c.response();
            (w.text(), w.header("connection").unwrap_or("").to_string())
        })
        .collect();
    let want = [("1", "keep-alive"), ("2", "keep-alive"), ("3", "close")].map(|(a, b)| (a.to_string(), b.to_string())).to_vec();
    check!("three of four", got, want);
    check!("then closed, the fourth unanswered", c.everything(), Vec::new());
}

#[test]
fn http_1_0_closes_unless_it_asks_to_stay() {
    let server = start(Config::default());
    check!("plain 1.0", exchange(server.addr(), b"GET /echo/a HTTP/1.0\r\n\r\n").header("connection").map(str::to_string), Some("close".to_string()));
    let mut c = Client::connect(server.addr());
    for _ in 0..2 {
        c.send(b"GET /echo/b HTTP/1.0\r\nConnection: Keep-Alive\r\n\r\n");
        check!("1.0 with keep-alive", c.response().header("connection").map(str::to_string), Some("keep-alive".to_string()));
    }
}

#[test]
fn close_is_found_among_other_tokens() {
    let server = start(Config::default());
    let w = exchange(server.addr(), b"GET /echo/a HTTP/1.1\r\nHost: t\r\nConnection: TE, close\r\n\r\n");
    check!("TE, close", w.header("connection").map(str::to_string), Some("close".to_string()));
}

#[test]
fn head_and_streams_leave_the_connection_usable() {
    let server = start(Config::default());
    let mut c = Client::connect(server.addr());
    c.send(b"HEAD /echo/abc HTTP/1.1\r\nHost: t\r\n\r\n");
    check!("HEAD", c.head_response().header("content-length").map(str::to_string), Some("3".to_string()));
    c.send(&get("/count"));
    check!("stream", c.response().text(), "0;1;2;".to_string());
    c.send(&get("/echo/after"));
    check!("afterwards", c.response().text(), "after".to_string());
}

#[test]
fn a_bad_request_in_a_pipeline_ends_the_connection_after_the_good_ones() {
    let server = start(Config::default());
    let mut c = Client::connect(server.addr());
    c.send(&[get("/echo/good"), b"BAD\r\n\r\n".to_vec(), get("/echo/never")].concat());
    check!("first", c.response().text(), "good".to_string());
    let w = c.response();
    check!("then the refusal", (w.status, w.header("connection").map(str::to_string)), (400, Some("close".to_string())));
    check!("and nothing more", c.everything(), Vec::new());
}

#[test]
fn continue_is_sent_once_however_the_body_arrives() {
    let server = start(Config::default());
    let mut c = Client::connect(server.addr());
    for _ in 0..2 {
        c.send(b"POST /echo HTTP/1.1\r\nHost: t\r\nContent-Length: 4\r\nExpect: 100-continue\r\n\r\n");
        check!("interim", c.response().status, 100);
        for b in b"data" {
            c.send(&[*b]);
            std::thread::sleep(Duration::from_millis(10));
        }
        let w = c.response();
        check!("the next response is the final one", (w.status, w.text()), (200, "data".to_string()));
    }
}

#[test]
fn no_continue_when_the_body_came_with_the_head() {
    let server = start(Config::default());
    let mut c = Client::connect(server.addr());
    c.send(b"POST /echo HTTP/1.1\r\nHost: t\r\nContent-Length: 2\r\nExpect: 100-continue\r\n\r\nok");
    check!("final reply directly", c.response().status, 200);
}

#[test]
fn a_request_split_after_a_pipelined_one() {
    let server = start(Config::default());
    let mut c = Client::connect(server.addr());
    let second = get("/echo/second");
    c.send(&[get("/echo/first"), second[..10].to_vec()].concat());
    check!("first is answered without waiting for the rest", c.response().text(), "first".to_string());
    c.send(&second[10..]);
    check!("second", c.response().text(), "second".to_string());
}

#[test]
fn random_pipelines_split_anywhere() {
    let server = start(Config::default());
    let mut rng = anneal_prelude::Rng::new(81007);
    for _ in 0..30 {
        let count = 1 + rng.below(6);
        let mut bytes = Vec::new();
        let mut want = Vec::new();
        for _ in 0..count {
            let len = 1 + rng.below(30);
            let word = rng.string(len, "abcxyz019");
            if rng.bool() {
                bytes.extend(get(&format!("/echo/{word}")));
            } else {
                bytes.extend(format!("POST /echo HTTP/1.1\r\nHost: t\r\nContent-Length: {len}\r\n\r\n{word}").into_bytes());
            }
            want.push(word);
        }
        let mut c = Client::connect(server.addr());
        let mut at = 0;
        while at < bytes.len() {
            let n = (1 + rng.below(50)).min(bytes.len() - at);
            c.send(&bytes[at..at + n]);
            at += n;
        }
        let got: Vec<String> = (0..count).map(|_| c.response().text()).collect();
        check!(format!("{count} requests"), got, want);
    }
}
