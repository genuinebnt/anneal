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

fn app(gate: Gate) -> App {
    let mut r = Router::new();
    r.add("GET", "/hello", |_, _| Response::text(200, "hello"));
    r.add("GET", "/echo/:word", |_, p| Response::text(200, p.get("word").unwrap_or("").to_string()));
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
fn exactly_the_queue_waits_and_the_rest_are_refused() {
    let gate = Gate::default();
    let server = start(2, 3, &gate);
    let mut busy = Vec::new();
    for n in 1..=2 {
        busy.push(ask(server.addr(), "/block"));
        gate.wait_for(n);
    }
    let waiting: Vec<Client> = (0..3).map(|i| ask(server.addr(), &format!("/echo/w{i}"))).collect();
    for i in 0..4 {
        check!(format!("extra {i}"), reply(ask(server.addr(), "/hello")).status, 503);
    }
    gate.open();
    for c in busy {
        check!("busy", reply(c).status, 200);
    }
    for (i, c) in waiting.into_iter().enumerate() {
        check!(format!("waiting {i}"), reply(c).text(), format!("w{i}"));
    }
}

#[test]
fn a_refused_client_that_had_already_sent_still_reads_the_whole_reply() {
    let gate = Gate::default();
    let server = start(1, 1, &gate);
    let busy = ask(server.addr(), "/block");
    gate.wait_for(1);
    let waiting = ask(server.addr(), "/hello");
    for _ in 0..8 {
        let mut c = Client::connect(server.addr());
        let mut bytes = b"GET /hello HTTP/1.1\r\nHost: t\r\nX: ".to_vec();
        bytes.extend_from_slice(&[b'y'; 30_000]);
        let _ = c.try_send(&bytes);
        check!("503 then a clean end", reply(c).status, 503);
    }
    gate.open();
    drop((reply(busy), reply(waiting)));
}

#[test]
fn a_refused_client_that_sends_late_still_reads_the_reply() {
    let gate = Gate::default();
    let server = start(1, 1, &gate);
    let busy = ask(server.addr(), "/block");
    gate.wait_for(1);
    let waiting = ask(server.addr(), "/hello");
    for _ in 0..5 {
        let mut c = Client::connect(server.addr());
        // wait until the refusal is on its way, then send the request anyway
        c.reader.fill().expect("the first bytes of the refusal");
        let _ = c.try_send(b"GET /hello HTTP/1.1\r\nHost: t\r\n\r\n");
        let _ = c.try_send(b"more");
        check!("503 then a clean end", reply(c).status, 503);
    }
    gate.open();
    drop((reply(busy), reply(waiting)));
}

#[test]
fn silent_refused_clients_do_not_stop_others_being_answered() {
    let gate = Gate::default();
    let server = start(1, 1, &gate);
    let busy = ask(server.addr(), "/block");
    gate.wait_for(1);
    let waiting = ask(server.addr(), "/hello");
    // these never send and never close
    let mut silent: Vec<Client> = (0..6).map(|_| Client::connect(server.addr())).collect();
    for c in &mut silent {
        check!("each is told", c.response().status, 503);
    }
    gate.open();
    drop((reply(busy), reply(waiting)));
    check!("and the server still serves", fetch(server.addr(), "/hello").status, 200);
}

#[test]
fn many_panics_leave_every_worker() {
    let gate = Gate::default();
    let server = start(3, 16, &gate);
    for _ in 0..12 {
        drop(ask(server.addr(), "/boom").everything());
    }
    let all: Vec<Client> = (0..3).map(|_| ask(server.addr(), "/block")).collect();
    gate.wait_for(3);
    gate.open();
    for c in all {
        check!("released", reply(c).status, 200);
    }
}

#[test]
fn many_clients_each_get_their_own_answer() {
    let gate = Gate::default();
    let server = start(4, 64, &gate);
    let addr = server.addr();
    let threads: Vec<_> = (0..8)
        .map(|t| std::thread::spawn(move || (0..30).all(|i| fetch(addr, &format!("/echo/{t}x{i}")).text() == format!("{t}x{i}"))))
        .collect();
    for (t, h) in threads.into_iter().enumerate() {
        check!(format!("client thread {t}"), h.join().unwrap(), true);
    }
}

#[test]
fn the_error_reply_still_survives_unread_input() {
    let gate = Gate::default();
    let server = start(2, 4, &gate);
    let mut c = Client::connect(server.addr());
    let mut bytes = b"GET / HTTP/1.1\r\nHost: t\r\nX-Big: ".to_vec();
    bytes.extend_from_slice(&[b'b'; 60_000]);
    let _ = c.try_send(&bytes);
    check!("431", reply(c).status, 431);
}

#[test]
fn queued_connections_are_served_even_when_one_before_them_panics() {
    let gate = Gate::default();
    let server = start(1, 8, &gate);
    let busy = ask(server.addr(), "/block");
    gate.wait_for(1);
    let boom = ask(server.addr(), "/boom");
    let after: Vec<Client> = (0..3).map(|i| ask(server.addr(), &format!("/echo/{i}"))).collect();
    gate.open();
    drop((reply(busy), boom.everything()));
    for (i, c) in after.into_iter().enumerate() {
        check!(format!("after the panic {i}"), reply(c).text(), i.to_string());
    }
}

#[test]
fn one_worker_and_a_deep_queue() {
    let gate = Gate::default();
    gate.open();
    let server = start(1, 32, &gate);
    let all: Vec<Client> = (0..20).map(|i| ask(server.addr(), &format!("/echo/{i}"))).collect();
    for (i, c) in all.into_iter().enumerate() {
        check!(format!("{i}"), reply(c).text(), i.to_string());
    }
}
