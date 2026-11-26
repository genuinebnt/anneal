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

fn app() -> App {
    let mut r = Router::new();
    r.add("GET", "/hello", |_, _| Response::text(200, "hello"));
    r.add("POST", "/echo", |req, _| Response::new(200).bytes(req.body.clone()));
    r.add("GET", "/count", |_, _| Response::new(200).stream((0..3).map(|i| format!("{i};").into_bytes())));
    r.add("GET", "/boom", |_, _| panic!("handler bug"));
    App::new(r)
}

fn start() -> Server {
    Server::start(Config::default(), app()).expect("start")
}

#[test]
fn a_large_body_arrives_whole() {
    let server = start();
    let body: Vec<u8> = (0..60_000u32).map(|i| (i % 251) as u8).collect();
    let mut bytes = format!("POST /echo HTTP/1.1\r\nHost: t\r\nConnection: close\r\nContent-Length: {}\r\n\r\n", body.len()).into_bytes();
    bytes.extend_from_slice(&body);
    check!("60000 bytes there and back", exchange(server.addr(), &bytes).body == body, true);
}

#[test]
fn a_chunked_upload_sent_in_parts() {
    let server = start();
    let mut c = Client::connect(server.addr());
    for part in ["POST /echo HTTP/1.1\r\nHost: t\r\nConnection: close\r\nTransfer-Encoding: chunked\r\n\r\n", "5\r\nhel", "lo\r\n6\r\n world\r\n", "0\r\n\r\n"] {
        c.send(part.as_bytes());
    }
    check!("chunked", decode(&c.everything(), false).text(), "hello world".to_string());
}

#[test]
fn a_streamed_response() {
    let server = start();
    let w = fetch(server.addr(), "/count");
    check!("chunked reply", (w.header("transfer-encoding"), w.text()), (Some("chunked"), "0;1;2;".to_string()));
}

#[test]
fn an_http_1_0_client_gets_no_chunks() {
    let server = start();
    let w = exchange(server.addr(), b"GET /count HTTP/1.0\r\n\r\n");
    check!("HTTP/1.0 and a stream", (w.header("transfer-encoding"), w.header("content-length"), w.text()), (None, Some("6"), "0;1;2;".to_string()));
}

#[test]
fn the_error_reply_survives_unread_input() {
    let server = start();
    for _ in 0..5 {
        let mut c = Client::connect(server.addr());
        let mut bytes = b"GET / HTTP/1.1\r\nHost: t\r\nX-Big: ".to_vec();
        bytes.extend_from_slice(&[b'b'; 60_000]);
        bytes.extend_from_slice(b"\r\n\r\n");
        // the server may refuse long before all of this is read
        let _ = c.try_send(&bytes);
        check!("431 then a clean end", decode(&c.everything(), false).status, 431);
    }
}

#[test]
fn a_reply_survives_bytes_sent_after_the_request() {
    let server = start();
    for _ in 0..5 {
        let mut c = Client::connect(server.addr());
        let mut bytes = b"GET /hello HTTP/1.1\r\nHost: t\r\nConnection: close\r\n\r\n".to_vec();
        bytes.extend_from_slice(&[b'z'; 60_000]);
        let _ = c.try_send(&bytes);
        check!("200 then a clean end", decode(&c.everything(), false).status, 200);
    }
}

#[test]
fn a_client_that_leaves_early_does_not_stop_the_server() {
    let server = start();
    drop(Client::connect(server.addr()));
    let mut half = Client::connect(server.addr());
    half.send(b"GET /hel");
    drop(half);
    let mut quiet = Client::connect(server.addr());
    quiet.send(b"GET /hello HTTP/1.1\r\n");
    quiet.finish_sending();
    check!("an unfinished request gets no reply", quiet.everything(), Vec::new());
    check!("the next client", fetch(server.addr(), "/hello").status, 200);
}

#[test]
fn a_panicking_handler_does_not_stop_the_server() {
    let server = start();
    let mut c = Client::connect(server.addr());
    c.send(b"GET /boom HTTP/1.1\r\nHost: t\r\nConnection: close\r\n\r\n");
    drop(c.everything());
    check!("after the panic", fetch(server.addr(), "/hello").status, 200);
}

#[test]
#[allow(clippy::needless_update)]
fn the_configured_limits_are_used() {
    let config = Config { limits: Limits { max_body: 4, ..Limits::default() }, ..Config::default() };
    let server = Server::start(config, app()).unwrap();
    check!("body over the limit", exchange(server.addr(), b"POST /echo HTTP/1.1\r\nHost: t\r\nContent-Length: 5\r\n\r\nhello").status, 413);
}

#[test]
fn the_server_listens_on_loopback() {
    let server = start();
    check!("address", (server.addr().ip().is_loopback(), server.addr().port() != 0), (true, true));
}

#[test]
fn two_servers_run_side_by_side() {
    let (a, b) = (start(), start());
    check!("different ports", a.addr() != b.addr(), true);
    check!("both serve", (fetch(a.addr(), "/hello").status, fetch(b.addr(), "/nope").status), (200, 404));
}

#[test]
fn random_requests_split_at_random_points() {
    let server = start();
    let mut rng = anneal_prelude::Rng::new(81006);
    for _ in 0..40 {
        let len = rng.below(200);
        let body: Vec<u8> = rng.vec(len, 0, 255);
        let mut bytes = format!("POST /echo HTTP/1.1\r\nHost: t\r\nConnection: close\r\nContent-Length: {len}\r\n\r\n").into_bytes();
        bytes.extend_from_slice(&body);
        let mut c = Client::connect(server.addr());
        let mut at = 0;
        while at < bytes.len() {
            let n = (1 + rng.below(60)).min(bytes.len() - at);
            c.send(&bytes[at..at + n]);
            at += n;
        }
        check!(format!("{len} bytes in pieces"), decode(&c.everything(), false).body, body);
    }
}
