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

mod step01 {
    use super::*;

    #[test]
    fn a_simple_get() {
        let r = whole(b"GET /hello HTTP/1.1\r\nHost: example\r\n\r\n");
        check!("GET /hello", (r.method.as_str(), r.path.as_str(), r.query.as_str(), r.minor, r.body.len()), ("GET", "/hello", "", 1, 0));
    }

    #[test]
    fn headers_are_lower_cased_and_trimmed() {
        let r = whole(b"GET / HTTP/1.1\r\nHost:  a.example \r\nX-Tag:\tone\r\n\r\n");
        check!("two headers", r.headers.clone(), vec![("host".to_string(), "a.example".to_string()), ("x-tag".to_string(), "one".to_string())]);
        check!("lookup in any case", (r.header("X-TAG"), r.header("x-tag"), r.header("missing")), (Some("one"), Some("one"), None));
    }

    #[test]
    fn the_query_is_split_off_at_the_first_question_mark() {
        let r = whole(b"GET /find?q=a?b&x=1 HTTP/1.1\r\nHost: h\r\n\r\n");
        check!("/find?q=a?b&x=1", (r.path.as_str(), r.query.as_str()), ("/find", "q=a?b&x=1"));
    }

    #[test]
    fn a_body_of_the_announced_length() {
        let r = whole(b"POST /up HTTP/1.1\r\nHost: h\r\nContent-Length: 5\r\n\r\nhello");
        check!("POST with 5 bytes", r.body, b"hello".to_vec());
    }

    #[test]
    fn a_request_line_without_a_version_is_bad() {
        bad(400, b"GET /\r\nHost: h\r\n\r\n");
    }

    #[test]
    fn http_1_1_needs_a_host() {
        bad(400, b"GET / HTTP/1.1\r\n\r\n");
    }
}

mod step02 {
    use super::*;

    const TWO: &[u8] = b"GET /a HTTP/1.1\r\nHost: h\r\n\r\nGET /b HTTP/1.1\r\nHost: h\r\n\r\n";

    #[test]
    fn half_a_request_is_incomplete() {
        check!("head without the blank line", parse(b"GET / HTTP/1.1\r\nHost: a\r\n"), Parsed::Incomplete { expect_continue: false });
    }

    #[test]
    fn pipelined_requests_report_the_bytes_used() {
        let used = match parse(TWO) {
            Parsed::Done(r, used) => (r.path, used),
            other => panic!("{other:?}"),
        };
        check!("two requests", used, ("/a".to_string(), 28));
    }

    #[test]
    fn a_long_request_line_is_414() {
        bad(414, format!("GET /{} HTTP/1.1\r\nHost: h\r\n\r\n", "a".repeat(3000)).as_bytes());
    }

    #[test]
    fn both_framings_at_once_are_400() {
        bad(400, b"POST / HTTP/1.1\r\nHost: h\r\nContent-Length: 3\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n");
    }

    #[test]
    fn a_chunked_body_is_joined() {
        let r = whole(b"POST / HTTP/1.1\r\nHost: h\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n6;x=1\r\n world\r\n0\r\nT: x\r\n\r\n");
        check!("two chunks and a trailer", r.body, b"hello world".to_vec());
    }

    #[test]
    fn a_huge_length_is_refused_before_the_body() {
        bad(413, b"POST / HTTP/1.1\r\nHost: h\r\nContent-Length: 10000000\r\n\r\n");
    }

    #[test]
    fn waiting_for_a_body_after_expect_continue() {
        check!(
            "head complete, body absent",
            parse(b"POST / HTTP/1.1\r\nHost: h\r\nContent-Length: 4\r\nExpect: 100-continue\r\n\r\n"),
            Parsed::Incomplete { expect_continue: true }
        );
    }
}

mod step03 {
    use super::*;

    fn send(response: Response, head_only: bool, keep_alive: bool) -> Vec<u8> {
        let mut out = Vec::new();
        let n = write_response(&mut out, response, head_only, keep_alive).expect("writing to a Vec");
        assert_eq!(n as usize, out.len(), "returned byte count");
        out
    }

    #[test]
    fn a_text_response() {
        let w = decode(&send(Response::text(200, "hi"), false, true), false);
        check!("200 text", (w.status, w.text(), w.header("content-length"), w.header("connection")), (200, "hi".to_string(), Some("2"), Some("keep-alive")));
    }

    #[test]
    fn the_status_line_has_the_reason() {
        let out = send(Response::new(404), false, false);
        check!("first line", String::from_utf8_lossy(&out).lines().next().map(str::to_string), Some("HTTP/1.1 404 Not Found".to_string()));
        check!("closing", decode(&out, false).header("connection").map(str::to_string), Some("close".to_string()));
    }

    #[test]
    fn head_sends_the_length_but_no_body() {
        let w = decode(&send(Response::text(200, "hello"), true, true), true);
        check!("HEAD", (w.header("content-length"), w.body.len()), (Some("5"), 0));
    }

    #[test]
    fn a_stream_is_sent_in_chunks() {
        let pieces = (0..3).map(|i| format!("line {i}\n").into_bytes());
        let w = decode(&send(Response::new(200).stream(pieces), false, true), false);
        check!("three pieces", (w.header("transfer-encoding"), w.header("content-length"), w.text()), (Some("chunked"), None, "line 0\nline 1\nline 2\n".to_string()));
    }

    #[test]
    fn error_responses_carry_their_status() {
        check!("431", decode(&send(error_response(HttpError::HeaderTooLarge), false, false), false).status, 431);
    }
}

mod step04 {
    use super::*;

    fn sample() -> Router {
        let mut r = Router::new();
        r.add("GET", "/", |_, _| Response::text(200, "root"));
        r.add("GET", "/hello", |_, _| Response::text(200, "hello"));
        r.add("GET", "/users/:id/posts/:post", |_, p| Response::text(200, format!("{} {}", p.get("id").unwrap_or("?"), p.get("post").unwrap_or("?"))));
        r.add("GET", "/files/*path", |_, p| Response::text(200, format!("[{}]", p.get("path").unwrap_or("?"))));
        r.add("POST", "/hello", |req, _| Response::new(200).bytes(req.body.clone()));
        r.add("PUT", "/only-put", |_, _| Response::new(204));
        r
    }

    fn body(r: &Response) -> String {
        match &r.body {
            Body::Bytes(b) => String::from_utf8_lossy(b).into_owned(),
            Body::Empty => String::new(),
            Body::Stream(_) => "<stream>".into(),
        }
    }

    fn get(r: &Router, path: &str) -> (u16, String) {
        let out = r.dispatch(&request("GET", path, &[]));
        (out.status, body(&out))
    }

    #[test]
    fn literal_routes() {
        check!("/ and /hello", (get(&sample(), "/"), get(&sample(), "/hello")), ((200, "root".to_string()), (200, "hello".to_string())));
    }

    #[test]
    fn path_values_are_captured() {
        check!("/users/7/posts/42", get(&sample(), "/users/7/posts/42"), (200, "7 42".to_string()));
    }

    #[test]
    fn an_unknown_path_is_404() {
        check!("/nope", get(&sample(), "/nope").0, 404);
    }

    #[test]
    fn a_known_path_with_another_method_is_405() {
        let out = sample().dispatch(&request("DELETE", "/hello", &[]));
        check!("DELETE /hello", (out.status, out.header_value("allow").map(str::to_string)), (405, Some("GET, POST".to_string())));
    }

    #[test]
    fn a_panic_becomes_500() {
        let mut r = Router::new();
        r.add("GET", "/boom", |_, _| panic!("handler bug"));
        let app = App::new(r).wrap(catch_panics());
        check!("/boom", app.handle(&request("GET", "/boom", &[])).status, 500);
    }

    #[test]
    fn request_ids_are_kept_or_made() {
        let app = App::new(sample()).wrap(request_ids());
        let kept = app.handle(&request("GET", "/hello", &[("X-Request-Id", "abc-1_2")]));
        check!("the client's id", kept.header_value("x-request-id").map(str::to_string), Some("abc-1_2".to_string()));
        let made = app.handle(&request("GET", "/hello", &[]));
        check!("a new id", made.header_value("x-request-id").map(|v| v.starts_with("req-")), Some(true));
    }
}

mod step05 {
    use super::*;

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
    fn serves_a_request() {
        let server = start();
        let w = fetch(server.addr(), "/hello");
        check!("GET /hello", (w.status, w.text(), w.header("connection")), (200, "hello".to_string(), Some("close")));
    }

    #[test]
    fn serves_one_client_after_another() {
        let server = start();
        for _ in 0..5 {
            check!("again", fetch(server.addr(), "/hello").status, 200);
        }
    }

    #[test]
    fn a_request_that_arrives_in_pieces() {
        let server = start();
        let mut c = Client::connect(server.addr());
        for byte in b"GET /hello HTTP/1.1\r\nHost: t\r\nConnection: close\r\n\r\n" {
            c.send(&[*byte]);
        }
        check!("one byte per write", decode(&c.everything(), false).text(), "hello".to_string());
    }

    #[test]
    fn a_bad_request_gets_its_status() {
        let server = start();
        check!("garbage", exchange(server.addr(), b"BOGUS\r\n\r\n").status, 400);
    }

    #[test]
    fn head_has_no_body() {
        let server = start();
        let mut c = Client::connect(server.addr());
        c.send(b"HEAD /hello HTTP/1.1\r\nHost: t\r\nConnection: close\r\n\r\n");
        let w = decode(&c.everything(), true);
        check!("HEAD /hello", (w.status, w.header("content-length")), (200, Some("5")));
    }
}

mod step06 {
    use super::*;

    fn app(gate: Gate) -> App {
        let mut r = Router::new();
        r.add("GET", "/hello", |_, _| Response::text(200, "hello"));
        r.add("GET", "/block", move |_, _| {
            gate.pass();
            Response::text(200, "released")
        });
        r.add("GET", "/boom", |_, _| panic!("handler bug"));
        App::new(r)
    }

    fn start(workers: usize, queue_cap: usize, gate: &Gate) -> Server {
        Server::start(Config { workers, queue_cap, ..Config::default() }, app(gate.clone())).expect("start")
    }

    fn ask(addr: std::net::SocketAddr, path: &str) -> Client {
        let mut c = Client::connect(addr);
        c.send(format!("GET {path} HTTP/1.1\r\nHost: t\r\nConnection: close\r\n\r\n").as_bytes());
        c
    }

    fn reply(c: Client) -> Wire {
        decode(&c.everything(), false)
    }

    #[test]
    fn a_slow_request_does_not_delay_another() {
        let gate = Gate::default();
        let server = start(2, 4, &gate);
        let slow = ask(server.addr(), "/block");
        gate.wait_for(1);
        check!("served while the first is still inside", fetch(server.addr(), "/hello").status, 200);
        gate.open();
        check!("the slow one", reply(slow).text(), "released".to_string());
    }

    #[test]
    fn every_worker_runs_at_once() {
        let gate = Gate::default();
        let server = start(3, 4, &gate);
        let all: Vec<Client> = (0..3).map(|_| ask(server.addr(), "/block")).collect();
        gate.wait_for(3);
        gate.open();
        for c in all {
            check!("released", reply(c).status, 200);
        }
    }

    #[test]
    fn a_full_server_says_so() {
        let gate = Gate::default();
        let server = start(1, 1, &gate);
        let busy = ask(server.addr(), "/block");
        gate.wait_for(1);
        let waiting = ask(server.addr(), "/hello");
        let w = reply(ask(server.addr(), "/hello"));
        check!("the third", (w.status, w.header("retry-after"), w.header("connection")), (503, Some("1"), Some("close")));
        gate.open();
        check!("accepted work is done", (reply(busy).status, reply(waiting).status), (200, 200));
    }

    #[test]
    fn after_the_rush_it_serves_again() {
        let gate = Gate::default();
        let server = start(1, 1, &gate);
        let busy = ask(server.addr(), "/block");
        gate.wait_for(1);
        let waiting = ask(server.addr(), "/hello");
        check!("full", reply(ask(server.addr(), "/hello")).status, 503);
        gate.open();
        drop((reply(busy), reply(waiting)));
        check!("free again", fetch(server.addr(), "/hello").status, 200);
    }

    #[test]
    fn a_panic_does_not_cost_a_worker() {
        let gate = Gate::default();
        let server = start(1, 4, &gate);
        for _ in 0..3 {
            drop(ask(server.addr(), "/boom").everything());
        }
        check!("the only worker is alive", fetch(server.addr(), "/hello").status, 200);
    }
}

mod step07 {
    use super::*;

    use std::time::Duration;

    fn app() -> App {
        let mut r = Router::new();
        r.add("GET", "/echo/:word", |_, p| Response::text(200, p.get("word").unwrap_or("").to_string()));
        r.add("POST", "/echo", |req, _| Response::new(200).bytes(req.body.clone()));
        App::new(r)
    }

    fn start(config: Config) -> Server {
        Server::start(config, app()).expect("start")
    }

    fn get(path: &str) -> Vec<u8> {
        format!("GET {path} HTTP/1.1\r\nHost: t\r\n\r\n").into_bytes()
    }

    #[test]
    fn several_requests_on_one_connection() {
        let server = start(Config::default());
        let mut c = Client::connect(server.addr());
        for i in 0..5 {
            c.send(&get(&format!("/echo/{i}")));
            let w = c.response();
            check!(format!("request {i}"), (w.text(), w.header("connection").map(str::to_string)), (i.to_string(), Some("keep-alive".to_string())));
        }
    }

    #[test]
    fn pipelined_requests_are_answered_in_order() {
        let server = start(Config::default());
        let mut c = Client::connect(server.addr());
        c.send(&[get("/echo/one"), get("/echo/two"), get("/echo/three")].concat());
        let got: Vec<String> = (0..3).map(|_| c.response().text()).collect();
        check!("three in one write", got, vec!["one".to_string(), "two".to_string(), "three".to_string()]);
    }

    #[test]
    fn connection_close_is_honoured() {
        let server = start(Config::default());
        let mut c = Client::connect(server.addr());
        c.send(b"GET /echo/bye HTTP/1.1\r\nHost: t\r\nConnection: Close\r\n\r\n");
        check!("says close", c.response().header("connection").map(str::to_string), Some("close".to_string()));
        check!("and closes", c.closed(), true);
    }

    #[test]
    fn an_idle_connection_is_closed_quietly() {
        let server = start(Config { idle_timeout: Duration::from_millis(250), ..Config::default() });
        let mut c = Client::connect(server.addr());
        c.send(&get("/echo/x"));
        check!("served", c.response().status, 200);
        check!("closed without another response", c.closed(), true);
    }

    #[test]
    fn half_a_request_times_out_with_408() {
        let server = start(Config { request_timeout: Duration::from_millis(250), ..Config::default() });
        let mut c = Client::connect(server.addr());
        c.send(b"GET /echo/x HTTP/1.1\r\nX-Slow: ");
        check!("408", c.response().status, 408);
        check!("then closed", c.closed(), true);
    }

    #[test]
    fn continue_is_sent_before_the_body() {
        let server = start(Config::default());
        let mut c = Client::connect(server.addr());
        c.send(b"POST /echo HTTP/1.1\r\nHost: t\r\nContent-Length: 5\r\nExpect: 100-continue\r\n\r\n");
        check!("interim reply", c.response().status, 100);
        c.send(b"hello");
        check!("final reply", c.response().text(), "hello".to_string());
    }
}

mod step08 {
    use super::*;

    /// A folder of files for one test, removed when dropped.
    struct Site {
        base: std::path::PathBuf,
        root: std::path::PathBuf,
    }

    impl Site {
        fn new() -> Site {
            static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
            let base = std::env::temp_dir().join(format!("site-{}-{n}-{nanos}", std::process::id()));
            let root = base.join("public");
            std::fs::create_dir_all(root.join("sub")).unwrap();
            std::fs::create_dir_all(base.join("public-private")).unwrap();
            std::fs::write(root.join("index.txt"), "hello from a file\n").unwrap();
            std::fs::write(root.join("page.html"), "<p>hi</p>").unwrap();
            std::fs::write(root.join("sub/nested.json"), "{}").unwrap();
            std::fs::write(root.join("with space.bin"), [1u8, 2, 3]).unwrap();
            std::fs::write(root.join("big.bin"), Site::big()).unwrap();
            std::fs::write(root.join("empty.bin"), "").unwrap();
            std::fs::write(base.join("secret.txt"), "TOP SECRET").unwrap();
            std::fs::write(base.join("public-private/key.txt"), "TOP SECRET").unwrap();
            std::os::unix::fs::symlink(base.join("secret.txt"), root.join("escape.txt")).unwrap();
            std::os::unix::fs::symlink(base.join("public-private"), root.join("neighbour")).unwrap();
            std::os::unix::fs::symlink(root.join("index.txt"), root.join("alias.txt")).unwrap();
            Site { base, root }
        }

        fn big() -> Vec<u8> {
            (0..1000u32).map(|i| (i % 251) as u8).collect()
        }

        fn get(&self, raw: &str, headers: &[(&str, &str)]) -> Response {
            serve_static(&self.root, raw, &request("GET", "/static", headers))
        }
    }

    impl Drop for Site {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.base);
        }
    }

    fn data(r: &Response) -> Vec<u8> {
        match &r.body {
            Body::Bytes(b) => b.clone(),
            Body::Empty => Vec::new(),
            Body::Stream(_) => panic!("unexpected stream"),
        }
    }

    fn header(r: &Response, name: &str) -> String {
        r.header_value(name).unwrap_or("").to_string()
    }

    #[test]
    fn a_file_is_served_with_validators() {
        let site = Site::new();
        let r = site.get("index.txt", &[]);
        check!("index.txt", (r.status, data(&r), header(&r, "content-type"), header(&r, "accept-ranges")), (200, b"hello from a file\n".to_vec(), "text/plain; charset=utf-8".to_string(), "bytes".to_string()));
        check!("quoted ETag and a GMT date", (header(&r, "etag").starts_with('"'), header(&r, "last-modified").ends_with("GMT")), (true, true));
    }

    #[test]
    fn a_parent_segment_is_forbidden() {
        let site = Site::new();
        check!("../secret.txt", site.get("../secret.txt", &[]).status, 403);
    }

    #[test]
    fn a_link_leading_outside_is_forbidden() {
        let site = Site::new();
        check!("escape.txt", site.get("escape.txt", &[]).status, 403);
    }

    #[test]
    fn a_matching_tag_gives_304() {
        let site = Site::new();
        let tag = header(&site.get("index.txt", &[]), "etag");
        let r = site.get("index.txt", &[("If-None-Match", &tag)]);
        check!("If-None-Match", (r.status, data(&r).len()), (304, 0));
    }

    #[test]
    fn a_range_gives_part_of_the_file() {
        let site = Site::new();
        let r = site.get("big.bin", &[("Range", "bytes=0-9")]);
        check!("first ten bytes", (r.status, data(&r), header(&r, "content-range")), (206, Site::big()[..10].to_vec(), "bytes 0-9/1000".to_string()));
    }

    #[test]
    fn through_the_server() {
        let site = Site::new();
        let mut router = Router::new();
        let root = site.root.clone();
        router.add("GET", "/static/*path", move |req, p| serve_static(&root, p.get("path").unwrap_or(""), req));
        let server = Server::start(Config::default(), App::new(router)).unwrap();
        check!("nested file", fetch(server.addr(), "/static/sub/nested.json").text(), "{}".to_string());
        check!("outside", fetch(server.addr(), "/static/%2e%2e/secret.txt").status, 403);
    }
}

mod step09 {
    use super::*;

    use std::sync::Arc;

    /// The value on the line that starts with `name` and a space.
    fn value(report: &str, name: &str) -> Option<u64> {
        report.lines().find_map(|l| l.strip_prefix(name)?.strip_prefix(' ')?.trim().parse().ok())
    }

    fn start(workers: usize) -> (Server, Arc<Metrics>) {
        let metrics = Arc::new(Metrics::default());
        let mut r = Router::new();
        r.add("GET", "/hello", |_, _| Response::text(200, "hello"));
        r.add("GET", "/metrics", metrics_handler(metrics.clone()));
        let server = Server::start(Config { workers, metrics: metrics.clone(), ..Config::default() }, App::new(r)).unwrap();
        (server, metrics)
    }

    #[test]
    fn small_values_are_exact() {
        let h = Histogram::default();
        for v in 1..=10 {
            h.record(v);
        }
        check!("1 to 10", (h.count(), h.sum(), h.quantile(0.5), h.quantile(1.0)), (10, 55, 5, 10));
    }

    #[test]
    fn large_quantiles_are_close() {
        let h = Histogram::default();
        for v in 1..=10_000 {
            h.record(v);
        }
        let p99 = h.quantile(0.99);
        check!(format!("p99 = {p99}, wanted 9900 to 11137"), (9_900..=11_137).contains(&p99), true);
    }

    #[test]
    fn the_report_counts_requests_by_class() {
        let (server, _) = start(4);
        for path in ["/hello", "/hello", "/nope"] {
            fetch(server.addr(), path);
        }
        let report = fetch(server.addr(), "/metrics").text();
        check!("requests", value(&report, "http_requests_total"), Some(3));
        check!("classes", (value(&report, "http_responses_total{class=\"2xx\"}"), value(&report, "http_responses_total{class=\"4xx\"}")), (Some(2), Some(1)));
    }

    #[test]
    fn reused_connections_are_counted() {
        let (server, metrics) = start(4);
        let mut c = Client::connect(server.addr());
        for _ in 0..4 {
            c.send(b"GET /hello HTTP/1.1\r\nHost: t\r\n\r\n");
            c.response();
        }
        let report = metrics.render();
        check!("one connection, four requests", (value(&report, "http_connections_accepted_total"), value(&report, "http_requests_reused_total")), (Some(1), Some(3)));
    }

    #[test]
    fn durations_form_a_histogram() {
        let (server, metrics) = start(4);
        for _ in 0..5 {
            fetch(server.addr(), "/hello");
        }
        let report = metrics.render();
        check!("count and +Inf", (value(&report, "http_request_duration_us_count"), value(&report, "http_request_duration_us_bucket{le=\"+Inf\"}")), (Some(5), Some(5)));
    }
}

mod step10 {
    use super::*;

    use std::sync::Arc;
    use std::time::{Duration, Instant};

    /// A server whose handlers hold `alive`: once every server thread has ended and the server is gone, nothing else does.
    fn start(config: Config, gate: Gate, alive: Arc<()>) -> Server {
        let mut r = Router::new();
        r.add("GET", "/hello", move |_, _| {
            let _held = &alive;
            Response::text(200, "hello")
        });
        r.add("GET", "/block", move |_, _| {
            gate.pass();
            Response::text(200, "released")
        });
        Server::start(config, App::new(r)).unwrap()
    }

    /// Runs `shutdown` elsewhere, so that a test can act while it is under way.
    fn shut_down(server: Server) -> std::sync::mpsc::Receiver<()> {
        let (done, finished) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            server.shutdown();
            let _ = done.send(());
        });
        finished
    }

    fn wait_until_refused(addr: std::net::SocketAddr) {
        let deadline = Instant::now() + Duration::from_secs(3);
        // only a refusal counts: a listener nobody accepts from fills its backlog, and then connects merely time out
        while !matches!(
            std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(200)),
            Err(e) if e.kind() == std::io::ErrorKind::ConnectionRefused
        ) {
            assert!(Instant::now() < deadline, "new connections are still accepted after shutdown was called");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn shutdown_returns_and_refuses_new_connections() {
        let server = start(Config::default(), Gate::default(), Arc::new(()));
        let addr = server.addr();
        check!("before", fetch(addr, "/hello").status, 200);
        check!("returns", shut_down(server).recv_timeout(Duration::from_secs(3)).is_ok(), true);
        wait_until_refused(addr);
    }

    #[test]
    fn a_request_in_flight_is_finished_and_says_close() {
        let gate = Gate::default();
        let server = start(Config::default(), gate.clone(), Arc::new(()));
        let addr = server.addr();
        let mut c = Client::connect(addr);
        c.send(b"GET /block HTTP/1.1\r\nHost: t\r\n\r\n");
        gate.wait_for(1);
        let finished = shut_down(server);
        wait_until_refused(addr);
        gate.open();
        let w = c.response();
        check!("in flight", (w.text(), w.header("connection").map(str::to_string)), ("released".to_string(), Some("close".to_string())));
        check!("then closed", c.closed(), true);
        check!("shutdown returns", finished.recv_timeout(Duration::from_secs(3)).is_ok(), true);
    }

    #[test]
    fn an_idle_connection_does_not_hold_it_up() {
        let server = start(Config { idle_timeout: Duration::from_secs(60), ..Config::default() }, Gate::default(), Arc::new(()));
        let mut c = Client::connect(server.addr());
        c.send(b"GET /hello HTTP/1.1\r\nHost: t\r\n\r\n");
        c.response();
        check!("returns promptly", shut_down(server).recv_timeout(Duration::from_secs(3)).is_ok(), true);
        check!("the idle client is closed", c.closed(), true);
    }

    #[test]
    fn everything_the_server_held_is_released() {
        let alive = Arc::new(());
        let server = start(Config { idle_timeout: Duration::from_secs(60), ..Config::default() }, Gate::default(), alive.clone());
        let mut c = Client::connect(server.addr());
        c.send(b"GET /hello HTTP/1.1\r\nHost: t\r\n\r\n");
        c.response();
        check!("returns", shut_down(server).recv_timeout(Duration::from_secs(3)).is_ok(), true);
        check!("owners of the handlers' data", Arc::strong_count(&alive), 1);
    }

    #[test]
    fn dropping_the_server_shuts_it_down() {
        let alive = Arc::new(());
        let server = start(Config::default(), Gate::default(), alive.clone());
        let addr = server.addr();
        drop(server);
        check!("released", Arc::strong_count(&alive), 1);
        wait_until_refused(addr);
    }
}
