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

use std::sync::Arc;
use std::time::{Duration, Instant};

fn value(report: &str, name: &str) -> Option<i64> {
    report.lines().find_map(|l| l.strip_prefix(name)?.strip_prefix(' ')?.trim().parse().ok())
}

/// Waits for a number that changes when the server finishes something the client cannot see.
fn settles(metrics: &Metrics, name: &str, want: i64) {
    let deadline = Instant::now() + PATIENCE;
    while value(&metrics.render(), name) != Some(want) {
        assert!(Instant::now() < deadline, "{name} never became {want}: {:?}", value(&metrics.render(), name));
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn start(config: Config, gate: Gate) -> (Server, Arc<Metrics>) {
    let metrics = config.metrics.clone();
    let mut r = Router::new();
    r.add("GET", "/hello", |_, _| Response::text(200, "hello"));
    r.add("GET", "/block", move |_, _| {
        gate.pass();
        Response::text(200, "released")
    });
    r.add("GET", "/boom", |_, _| panic!("x"));
    r.add("GET", "/metrics", metrics_handler(metrics.clone()));
    (Server::start(config, App::new(r).wrap(catch_panics())).unwrap(), metrics)
}

#[test]
fn nearest_rank_within_an_eighth_for_random_values() {
    let mut rng = anneal_prelude::Rng::new(81009);
    for _ in 0..200 {
        let h = Histogram::default();
        let n = 1 + rng.below(60);
        let scale = rng.below(60);
        let mut values: Vec<u64> = (0..n).map(|_| rng.next_u64() >> (4 + rng.below(1 + scale))).collect();
        for &v in &values {
            h.record(v);
        }
        values.sort_unstable();
        for q in [0.0, 0.25, 0.5, 0.9, 0.99, 1.0] {
            let rank = ((q * n as f64).ceil() as usize).max(1);
            let exact = values[rank - 1];
            let got = h.quantile(q);
            let top = exact.saturating_add(exact / 8).min(values[n - 1]);
            check!(format!("q={q} of {n} values, exact {exact}, got {got}"), got >= exact && got <= top, true);
        }
    }
}

#[test]
fn the_whole_range_of_u64_is_accepted() {
    let h = Histogram::default();
    for v in [0, 1, 15, 16, 1 << 40, 1 << 50, u64::MAX / 2, u64::MAX] {
        h.record(v);
    }
    check!("extremes", (h.quantile(1.0), h.quantile(0.0), h.count(), h.sum()), (u64::MAX, 0, 8, u64::MAX));
    let h = Histogram::default();
    h.record(1 << 50);
    h.record(u64::MAX);
    let low = h.quantile(0.5);
    check!(format!("a large value stays close: {low}"), (1u64 << 50..=(1u64 << 50) + (1 << 47)).contains(&low), true);
}

#[test]
fn an_empty_histogram() {
    check!("empty", (Histogram::default().quantile(0.5), Histogram::default().count()), (0, 0));
}

#[test]
fn recording_from_many_threads_loses_nothing() {
    let h = Arc::new(Histogram::default());
    let hs: Vec<_> = (0..4).map(|_| { let h = h.clone(); std::thread::spawn(move || for i in 0..20_000u64 { h.record(i % 500) }) }).collect();
    for t in hs {
        t.join().unwrap();
    }
    check!("count and sum", (h.count(), h.sum()), (80_000, 4 * 40 * (0..500u64).sum::<u64>()));
}

#[test]
fn buckets_are_cumulative_and_never_decrease() {
    let (server, metrics) = start(Config::default(), Gate::default());
    for _ in 0..20 {
        fetch(server.addr(), "/hello");
    }
    let report = metrics.render();
    let buckets: Vec<(String, i64)> = report
        .lines()
        .filter(|l| l.starts_with("http_request_duration_us_bucket"))
        .map(|l| {
            let (name, v) = l.rsplit_once(' ').unwrap();
            (name.to_string(), v.parse().unwrap())
        })
        .collect();
    let names: Vec<String> = ["100", "250", "500", "1000", "2500", "5000", "10000", "50000", "100000", "+Inf"].iter().map(|le| format!("http_request_duration_us_bucket{{le=\"{le}\"}}")).collect();
    check!("bucket lines in order", buckets.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>(), names);
    check!(format!("never decreasing: {buckets:?}"), buckets.windows(2).all(|w| w[0].1 <= w[1].1), true);
    check!("last is the count", buckets.last().map(|b| b.1), value(&report, "http_request_duration_us_count"));
    // handlers that only build a short string take far less than 100 ms
    check!("all within the largest bound", buckets[8].1, 20);
}

#[test]
fn a_slow_handler_lands_above_the_small_buckets() {
    let gate = Gate::default();
    let (server, metrics) = start(Config::default(), gate.clone());
    let mut c = Client::connect(server.addr());
    c.send(b"GET /block HTTP/1.1\r\nHost: t\r\n\r\n");
    gate.wait_for(1);
    std::thread::sleep(Duration::from_millis(15));
    gate.open();
    c.response();
    let report = metrics.render();
    check!("not in le=10000", value(&report, "http_request_duration_us_bucket{le=\"10000\"}"), Some(0));
    check!("sum is at least 15 ms", value(&report, "http_request_duration_us_sum").is_some_and(|s| s >= 15_000), true);
    check!("p99 too", value(&report, "http_request_duration_us_p99").is_some_and(|s| s >= 15_000), true);
}

#[test]
fn every_final_response_is_counted_including_refusals() {
    let config = Config { request_timeout: Duration::from_millis(200), ..Config::default() };
    let (server, metrics) = start(config, Gate::default());
    exchange(server.addr(), b"BAD\r\n\r\n");
    exchange(server.addr(), b"GET /slow HTTP/1.1\r\n");
    fetch(server.addr(), "/boom");
    let mut c = Client::connect(server.addr());
    c.send(b"POST /hello HTTP/1.1\r\nHost: t\r\nContent-Length: 1\r\nExpect: 100-continue\r\nConnection: close\r\n\r\n");
    check!("interim", c.response().status, 100);
    c.send(b"x");
    check!("405", c.response().status, 405);
    let report = metrics.render();
    check!("four final responses", value(&report, "http_requests_total"), Some(4));
    check!("4xx and 5xx", (value(&report, "http_responses_total{class=\"4xx\"}"), value(&report, "http_responses_total{class=\"5xx\"}"), value(&report, "http_responses_total{class=\"1xx\"}")), (Some(3), Some(1), Some(0)));
}

#[test]
fn gauges_follow_open_and_waiting_connections() {
    let gate = Gate::default();
    let (server, metrics) = start(Config { workers: 2, queue_cap: 2, ..Config::default() }, gate.clone());
    let mut busy = Vec::new();
    for n in 1..=2 {
        let mut c = Client::connect(server.addr());
        c.send(b"GET /block HTTP/1.1\r\nHost: t\r\nConnection: close\r\n\r\n");
        busy.push(c);
        gate.wait_for(n);
    }
    settles(&metrics, "http_queue_depth", 0);
    check!("two active", value(&metrics.render(), "http_connections_active"), Some(2));
    let waiting: Vec<Client> = (0..2)
        .map(|_| {
            let mut c = Client::connect(server.addr());
            c.send(b"GET /hello HTTP/1.1\r\nHost: t\r\nConnection: close\r\n\r\n");
            c
        })
        .collect();
    settles(&metrics, "http_queue_depth", 2);
    let refused = Client::connect(server.addr()).everything();
    check!("refused", decode(&refused, false).status, 503);
    let report = metrics.render();
    check!("one refusal", (value(&report, "http_connections_rejected_total"), value(&report, "http_connections_accepted_total"), value(&report, "http_requests_total"), value(&report, "http_responses_total{class=\"5xx\"}")), (Some(1), Some(5), Some(1), Some(1)));
    gate.open();
    for c in busy.into_iter().chain(waiting) {
        c.everything();
    }
    settles(&metrics, "http_connections_active", 0);
    settles(&metrics, "http_queue_depth", 0);
}

#[test]
fn the_report_does_not_count_the_request_that_asks_for_it() {
    let (server, _) = start(Config::default(), Gate::default());
    let first = fetch(server.addr(), "/metrics").text();
    let second = fetch(server.addr(), "/metrics").text();
    check!("0 then 1", (value(&first, "http_requests_total"), value(&second, "http_requests_total")), (Some(0), Some(1)));
}

#[test]
fn the_report_names_and_types() {
    let report = Metrics::default().render();
    for line in ["# TYPE http_requests_total counter", "# TYPE http_connections_active gauge", "# TYPE http_queue_depth gauge", "# TYPE http_request_duration_us histogram"] {
        check!(line, report.lines().any(|l| l == line), true);
    }
    let w = {
        let (server, _) = start(Config::default(), Gate::default());
        fetch(server.addr(), "/metrics")
    };
    check!("content type", w.header("content-type").map(str::to_string), Some("text/plain; version=0.0.4".to_string()));
}
