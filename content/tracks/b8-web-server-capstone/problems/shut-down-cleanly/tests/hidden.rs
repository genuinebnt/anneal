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
    fn http_1_0_needs_no_host() {
        check!("HTTP/1.0 without Host", whole(b"GET / HTTP/1.0\r\n\r\n").minor, 0);
    }

    #[test]
    fn request_lines_that_are_wrong() {
        for line in ["GET  / HTTP/1.1", "GET / HTTP/1.1 ", " GET / HTTP/1.1", "get / HTTP/1.1", "G3T / HTTP/1.1", " / HTTP/1.1", "GET hello HTTP/1.1", "GET / HTTP/2.0", "GET / http/1.1", "GET /", ""] {
            bad(400, format!("{line}\r\nHost: h\r\n\r\n").as_bytes());
        }
    }

    #[test]
    fn headers_that_are_wrong() {
        for line in ["no colon", ": empty name", "Bad Name: v", "Name : v", "N\tame: v"] {
            bad(400, format!("GET / HTTP/1.1\r\nHost: h\r\n{line}\r\n\r\n").as_bytes());
        }
    }

    #[test]
    fn a_value_may_hold_colons_and_inner_spaces() {
        let r = whole(b"GET / HTTP/1.1\r\nHost: a:80\r\nX-V:  one  two : three \r\n\r\n");
        check!("colons and spaces", (r.header("host"), r.header("x-v")), (Some("a:80"), Some("one  two : three")));
    }

    #[test]
    fn the_first_of_two_equal_names_wins_the_lookup_and_both_are_kept() {
        let r = whole(b"GET / HTTP/1.1\r\nHost: h\r\nX: 1\r\nx: 2\r\n\r\n");
        check!("repeated header", (r.header("X"), r.headers.len()), (Some("1"), 3));
    }

    #[test]
    fn a_binary_body_may_contain_blank_lines() {
        let body = [0u8, 255, 13, 10, 13, 10, 7];
        let mut bytes = b"POST / HTTP/1.1\r\nHost: h\r\nContent-Length: 7\r\n\r\n".to_vec();
        bytes.extend_from_slice(&body);
        check!("7 bytes of binary", whole(&bytes).body, body.to_vec());
    }

    #[test]
    fn a_head_that_is_not_utf8_is_bad() {
        bad(400, b"GET /\xff HTTP/1.1\r\nHost: h\r\n\r\n");
    }

    #[test]
    fn bytes_used_stops_at_the_end_of_the_first_request() {
        let two = b"POST /a HTTP/1.1\r\nHost: h\r\nContent-Length: 2\r\n\r\nhiGET /b HTTP/1.1\r\nHost: h\r\n\r\n";
        let first = match parse(two) {
            Parsed::Done(r, used) => (r.path, r.body, used),
            other => panic!("{other:?}"),
        };
        let first_len = two.len() - b"GET /b HTTP/1.1\r\nHost: h\r\n\r\n".len();
        check!("two requests in one buffer", first, ("/a".to_string(), b"hi".to_vec(), first_len));
    }

    #[test]
    fn status_codes_of_the_errors() {
        check!("statuses", [HttpError::BadRequest, HttpError::UriTooLong, HttpError::HeaderTooLarge, HttpError::BodyTooLarge].map(HttpError::status), [400, 414, 431, 413]);
    }

    #[test]
    fn random_well_formed_requests() {
        let mut rng = anneal_prelude::Rng::new(81001);
        for _ in 0..200 {
            let method = *rng.pick(&["GET", "POST", "PUT", "DELETE", "HEAD"]);
            let len = rng.below(8);
            let path = format!("/{}", rng.string(len, "ab/-._~"));
            let body_len = rng.below(20);
            let body: Vec<u8> = rng.vec(body_len, 0, 255);
            let mut bytes = format!("{method} {path} HTTP/1.1\r\nHost: h\r\n").into_bytes();
            let mut want = vec![("host".to_string(), "h".to_string())];
            for i in 0..rng.below(4) {
                let value_len = rng.below(6);
                let value = rng.string(value_len, "ab=;,: ").trim().to_string();
                bytes.extend_from_slice(format!("X-{i}: {value}\r\n").as_bytes());
                want.push((format!("x-{i}"), value));
            }
            if !body.is_empty() {
                bytes.extend_from_slice(format!("Content-Length: {}\r\n", body.len()).as_bytes());
                want.push(("content-length".to_string(), body.len().to_string()));
            }
            bytes.extend_from_slice(b"\r\n");
            bytes.extend_from_slice(&body);
            let r = whole(&bytes);
            check!(format!("{:?}", String::from_utf8_lossy(&bytes)), (r.method, r.path, r.headers, r.body), (method.to_string(), path, want, body));
        }
    }
}

mod step02 {
    use super::*;

    fn small() -> Limits {
        Limits { max_request_line: 30, max_header_bytes: 200, max_headers: 4, max_body: 16 }
    }

    /// A prefix of a good request is never refused.
    fn every_prefix_waits(bytes: &[u8], lim: &Limits) {
        for cut in 0..bytes.len() {
            let got = parse_request(&bytes[..cut], lim);
            assert!(matches!(got, Parsed::Incomplete { .. }), "first {cut} bytes of {:?} gave {got:?}", String::from_utf8_lossy(bytes));
        }
        let got = parse_request(bytes, lim);
        assert!(matches!(got, Parsed::Done(_, n) if n == bytes.len()), "{:?} gave {got:?}", String::from_utf8_lossy(bytes));
    }

    #[test]
    fn every_prefix_of_plain_and_chunked_requests_waits() {
        let lim = Limits::default();
        every_prefix_waits(b"GET /a?b HTTP/1.1\r\nHost: h\r\n\r\n", &lim);
        every_prefix_waits(b"POST / HTTP/1.1\r\nHost: h\r\nContent-Length: 4\r\n\r\n\r\n\r\n", &lim);
        every_prefix_waits(b"POST / HTTP/1.1\r\nHost: h\r\nTransfer-Encoding: Chunked\r\n\r\nA\r\n\r\n23456789\r\n1;name=value\r\nz\r\n0\r\nT: 1\r\nU: 2\r\n\r\n", &lim);
    }

    #[test]
    fn a_request_exactly_at_every_limit_arrives_byte_by_byte() {
        let lim = small();
        // 30 bytes of request line, four headers, a 16-byte body
        let line = format!("POST /{} HTTP/1.1", "a".repeat(30 - 15));
        assert_eq!(line.len(), 30);
        let mut bytes = format!("{line}\r\nHost: h\r\nA: 1\r\nB: 2\r\nContent-Length: 16\r\n\r\n").into_bytes();
        bytes.extend_from_slice(&[b'x'; 16]);
        every_prefix_waits(&bytes, &lim);
    }

    #[test]
    fn one_past_a_limit_is_refused() {
        let lim = small();
        let status = |b: &[u8]| match parse_request(b, &lim) {
            Parsed::Bad(e) => Some(e.status()),
            _ => None,
        };
        let long_line = format!("GET /{} HTTP/1.1\r\n", "a".repeat(31 - 14));
        check!("request line of 31 bytes", status(format!("{long_line}Host: h\r\n\r\n").as_bytes()), Some(414));
        check!("request line of 31 bytes, CRLF not here yet", status(&long_line.as_bytes()[..31]), Some(414));
        check!("five headers", status(b"GET / HTTP/1.1\r\nHost: h\r\nA: 1\r\nB: 1\r\nC: 1\r\nD: 1\r\n\r\n"), Some(431));
        check!("body of 17", status(b"POST / HTTP/1.1\r\nHost: h\r\nContent-Length: 17\r\n\r\n"), Some(413));
    }

    #[test]
    fn an_endless_head_is_refused_without_waiting_for_its_end() {
        let lim = Limits::default();
        let line = vec![b'a'; 5000];
        check!("5000 bytes and no line end", parse_request(&line, &lim), Parsed::Bad(HttpError::UriTooLong));
        let mut head = b"GET / HTTP/1.1\r\n".to_vec();
        while head.len() <= 9000 {
            head.extend_from_slice(b"X: yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy\r\n");
        }
        check!("9000 bytes of headers and no blank line", parse_request(&head, &lim), Parsed::Bad(HttpError::HeaderTooLarge));
    }

    #[test]
    fn ambiguous_lengths_are_400() {
        for headers in [
            "Content-Length: 1\r\nContent-Length: 2",
            "Content-Length: +3",
            "Content-Length: -1",
            "Content-Length: 3, 3",
            "Content-Length:",
            "Content-Length: 1x",
            "Transfer-Encoding: gzip",
            "Transfer-Encoding: gzip, chunked",
            "Transfer-Encoding: chunked\r\nTransfer-Encoding: chunked",
            "Transfer-Encoding: chunked\r\nTransfer-Encoding: identity",
            "X: 1\r\n folded",
            "X: 1\r\n\tfolded: yes",
        ] {
            bad(400, format!("POST / HTTP/1.1\r\nHost: h\r\n{headers}\r\n\r\n0\r\n\r\n").as_bytes());
        }
        bad(400, b"POST / HTTP/1.0\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n");
    }

    #[test]
    fn equal_repeated_lengths_are_accepted_and_numbers_too_big_are_413() {
        check!("same length twice", whole(b"POST / HTTP/1.1\r\nHost: h\r\nContent-Length: 2\r\nContent-Length: 2\r\n\r\nok").body, b"ok".to_vec());
        bad(413, b"POST / HTTP/1.1\r\nHost: h\r\nContent-Length: 99999999999999999999999\r\n\r\n");
    }

    #[test]
    fn chunks_that_are_wrong() {
        let head = "POST / HTTP/1.1\r\nHost: h\r\nTransfer-Encoding: chunked\r\n\r\n";
        for body in ["g\r\n", "+5\r\nhello\r\n0\r\n\r\n", " 5\r\nhello\r\n0\r\n\r\n", "5 \r\nhello\r\n0\r\n\r\n", "\r\n", ";a\r\n", "000000005\r\nhello\r\n0\r\n\r\n", "5\r\nhelloXX0\r\n\r\n", "3\r\nhello\r\n0\r\n\r\n"] {
            bad(400, format!("{head}{body}").as_bytes());
        }
    }

    #[test]
    fn a_long_chunk_line_is_refused_however_it_arrives() {
        let head = "POST / HTTP/1.1\r\nHost: h\r\nTransfer-Encoding: chunked\r\n\r\n";
        let ext = "e".repeat(200);
        bad(400, format!("{head}1;{ext}\r\nz\r\n0\r\n\r\n").as_bytes());
        bad(400, format!("{head}1;{ext}").as_bytes());
        // 64 bytes is still allowed, split anywhere
        every_prefix_waits(format!("{head}1;{}\r\nz\r\n0\r\n\r\n", "e".repeat(62)).as_bytes(), &Limits::default());
    }

    #[test]
    fn chunked_bodies_obey_the_body_limit_as_soon_as_a_size_is_announced() {
        let head = "POST / HTTP/1.1\r\nHost: h\r\nTransfer-Encoding: chunked\r\n\r\n";
        bad(413, format!("{head}FFFFFF\r\n").as_bytes());
        let lim = small();
        let two = format!("{head}10\r\n{}\r\n1\r\n", "a".repeat(16));
        check!("16 then 1 more with a limit of 16", parse_request(two.as_bytes(), &lim), Parsed::Bad(HttpError::BodyTooLarge));
    }

    #[test]
    fn endless_trailers_are_refused() {
        let mut bytes = b"POST / HTTP/1.1\r\nHost: h\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n".to_vec();
        for i in 0..2000 {
            bytes.extend_from_slice(format!("T{i}: padding-padding\r\n").as_bytes());
        }
        bad(431, &bytes);
    }

    #[test]
    fn expect_continue_is_reported_only_while_the_body_is_missing() {
        let head = b"POST / HTTP/1.1\r\nHost: h\r\nTransfer-Encoding: chunked\r\nExpect: 100-Continue\r\n\r\n";
        check!("chunked, nothing yet", parse(head), Parsed::Incomplete { expect_continue: true });
        check!("head incomplete", parse(&head[..head.len() - 1]), Parsed::Incomplete { expect_continue: false });
        bad(413, b"POST / HTTP/1.1\r\nHost: h\r\nContent-Length: 9999999\r\nExpect: 100-continue\r\n\r\n");
    }

    #[test]
    fn random_bytes_never_panic_and_refusals_are_final() {
        let mut rng = anneal_prelude::Rng::new(81002);
        let lim = small();
        let pieces: [&str; 14] = ["GET ", "POST ", "/", " HTTP/1.1", "\r\n", "\r", "\n", "Host: h", "Content-Length: ", "3", "Transfer-Encoding: chunked", ": ", "a", "0"];
        for _ in 0..400 {
            let mut bytes = Vec::new();
            let n = rng.below(40);
            for _ in 0..n {
                bytes.extend_from_slice(rng.pick(&pieces).as_bytes());
            }
            let mut first_refusal = None;
            for cut in 0..=bytes.len() {
                let got = parse_request(&bytes[..cut], &lim);
                if let Some(e) = first_refusal {
                    check!(format!("{:?}: refused earlier, now at {cut} bytes", String::from_utf8_lossy(&bytes)), got, Parsed::Bad(e));
                } else if let Parsed::Bad(e) = got {
                    first_refusal = Some(e);
                } else if let Parsed::Done(_, used) = got {
                    assert!(used <= cut);
                    break;
                }
            }
        }
    }

    #[test]
    fn random_chunkings_of_a_body_decode_to_the_same_bytes() {
        let mut rng = anneal_prelude::Rng::new(81003);
        for _ in 0..200 {
            let len = rng.below(60);
            let body: Vec<u8> = rng.vec(len, 0, 255);
            let mut bytes = b"POST / HTTP/1.1\r\nHost: h\r\nTransfer-Encoding: chunked\r\n\r\n".to_vec();
            let mut at = 0;
            while at < body.len() {
                let n = (1 + rng.below(20)).min(body.len() - at);
                let size = if rng.bool() { format!("{n:x}") } else { format!("{n:X}") };
                bytes.extend_from_slice(size.as_bytes());
                bytes.extend_from_slice(b"\r\n");
                bytes.extend_from_slice(&body[at..at + n]);
                bytes.extend_from_slice(b"\r\n");
                at += n;
            }
            bytes.extend_from_slice(b"0\r\n\r\n");
            check!(format!("body of {len} bytes"), whole(&bytes).body, body);
            let cut = rng.below(bytes.len());
            assert!(matches!(parse(&bytes[..cut]), Parsed::Incomplete { .. }), "cut at {cut}");
        }
    }
}

mod step03 {
    use super::*;

    use std::io::{self, Write};
    use std::sync::{Arc, Mutex};

    fn send(response: Response, head_only: bool) -> Vec<u8> {
        let mut out = Vec::new();
        let n = write_response(&mut out, response, head_only, false).expect("writing to a Vec");
        assert_eq!(n as usize, out.len(), "returned byte count");
        out
    }

    /// Accepts a few bytes per call, as a socket may.
    struct Trickle {
        out: Vec<u8>,
        step: usize,
    }

    impl Write for Trickle {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            let n = b.len().min(self.step);
            self.out.extend_from_slice(&b[..n]);
            Ok(n)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_writer_that_takes_a_few_bytes_at_a_time_gets_everything() {
        for step in [1, 3, 7] {
            let mut w = Trickle { out: Vec::new(), step };
            write_response(&mut w, Response::text(200, "0123456789".repeat(10)), false, false).unwrap();
            check!(format!("{step} bytes per write"), decode(&w.out, false).body.len(), 100);
            let mut w = Trickle { out: Vec::new(), step };
            write_response(&mut w, Response::new(200).stream(vec![b"abcdefgh".to_vec(); 3].into_iter()), false, false).unwrap();
            check!(format!("stream, {step} bytes per write"), decode(&w.out, false).body.len(), 24);
        }
    }

    #[derive(Clone, Default)]
    struct Log(Arc<Mutex<(Vec<u8>, Vec<u8>)>>);

    /// Keeps what was written apart from what has been flushed.
    impl Write for Log {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().0.extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            let mut s = self.0.lock().unwrap();
            let pending = std::mem::take(&mut s.0);
            s.1.extend(pending);
            Ok(())
        }
    }

    #[test]
    fn each_piece_is_flushed_before_the_next_one_is_asked_for() {
        let log = Log::default();
        let seen = log.clone();
        let pieces = (0..5).map(move |i| {
            let flushed = String::from_utf8_lossy(&seen.0.lock().unwrap().1).into_owned();
            for earlier in 0..i {
                assert!(flushed.contains(&format!("piece-{earlier}")), "piece {earlier} had not been flushed when piece {i} was asked for");
            }
            format!("piece-{i}").into_bytes()
        });
        let mut w = log.clone();
        write_response(&mut w, Response::new(200).stream(pieces), false, false).unwrap();
        let all = log.0.lock().unwrap().1.clone();
        check!("whole body", decode(&all, false).body.len(), 35);
    }

    struct Broken {
        room: usize,
    }

    impl Write for Broken {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            if self.room == 0 {
                return Err(io::Error::new(io::ErrorKind::BrokenPipe, "gone"));
            }
            let n = b.len().min(self.room);
            self.room -= n;
            Ok(n)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_failing_writer_stops_the_stream_and_reports_the_error() {
        let pulled = Arc::new(Mutex::new(0));
        let p = pulled.clone();
        let pieces = (0..100_000).map(move |_| {
            *p.lock().unwrap() += 1;
            vec![b'x'; 100]
        });
        let result = write_response(&mut Broken { room: 1000 }, Response::new(200).stream(pieces), false, false);
        check!("error kind", result.map_err(|e| e.kind()), Err(io::ErrorKind::BrokenPipe));
        let n = *pulled.lock().unwrap();
        assert!(n < 100, "{n} pieces were produced for a writer that failed after 1000 bytes");
    }

    #[test]
    fn a_failing_writer_on_a_plain_body_is_an_error() {
        check!("short writer", write_response(&mut Broken { room: 10 }, Response::text(200, "hello"), false, false).map_err(|e| e.kind()), Err(io::ErrorKind::BrokenPipe));
    }

    #[test]
    fn empty_pieces_do_not_end_the_body() {
        let pieces = vec![b"a".to_vec(), Vec::new(), b"b".to_vec(), Vec::new()];
        check!("empty pieces", decode(&send(Response::new(200).stream(pieces.into_iter()), false), false).text(), "ab".to_string());
    }

    #[test]
    fn an_empty_stream_is_still_a_complete_body() {
        check!("no pieces", decode(&send(Response::new(200).stream(Vec::<Vec<u8>>::new().into_iter()), false), false).body.len(), 0);
    }

    #[test]
    fn head_of_a_stream_sends_no_chunks() {
        let out = send(Response::new(200).stream(vec![b"secret".to_vec()].into_iter()), true);
        let w = decode(&out, true);
        check!("HEAD on a stream", (w.header("transfer-encoding"), out.ends_with(b"\r\n\r\n")), (Some("chunked"), true));
    }

    #[test]
    fn a_304_and_a_204_have_neither_body_nor_length() {
        for status in [304, 204] {
            let out = send(Response::new(status).header("ETag", "\"x\"").bytes(b"ignored".to_vec()), false);
            let w = decode(&out, false);
            check!(format!("{status}"), (w.header("content-length"), w.header("etag"), out.ends_with(b"\r\n\r\n")), (None, Some("\"x\""), true));
        }
    }

    #[test]
    fn headers_are_sent_in_order_and_repeats_are_kept() {
        let w = decode(&send(Response::new(200).header("Set-Cookie", "a=1").header("Set-Cookie", "b=2"), false), false);
        let cookies: Vec<&str> = w.headers.iter().filter(|(k, _)| k == "Set-Cookie").map(|(_, v)| v.as_str()).collect();
        check!("two cookies", (cookies, w.count("content-length"), w.count("connection")), (vec!["a=1", "b=2"], 1, 1));
    }

    #[test]
    fn lengths_count_bytes() {
        let w = decode(&send(Response::text(200, "héllo"), false), false);
        check!("non-ASCII text", (w.header("content-length"), w.text()), (Some("6"), "héllo".to_string()));
    }

    #[test]
    fn header_value_finds_in_any_case() {
        let r = Response::new(200).header("Content-Type", "a/b");
        check!("header_value", (r.header_value("content-type"), r.header_value("x")), (Some("a/b"), None));
    }

    #[test]
    fn random_bodies_survive_the_trip() {
        let mut rng = anneal_prelude::Rng::new(81004);
        for _ in 0..200 {
            let count = rng.below(5);
            let mut pieces = Vec::new();
            for _ in 0..count {
                let len = rng.below(300);
                pieces.push(rng.vec::<u8>(len, 0, 255));
            }
            let want = pieces.concat();
            let step = 1 + rng.below(50);
            let stream = rng.bool();
            let response = if stream { Response::new(200).stream(pieces.into_iter()) } else { Response::new(200).bytes(want.clone()) };
            let mut w = Trickle { out: Vec::new(), step };
            let n = write_response(&mut w, response, false, true).unwrap();
            check!(format!("{count} pieces, stream {stream}, {step} per write"), (decode(&w.out, false).body, n as usize), (want, w.out.len()));
        }
    }
}

mod step04 {
    use super::*;

    use std::sync::{Arc, Mutex};

    fn text(r: &Response) -> String {
        match &r.body {
            Body::Bytes(b) => String::from_utf8_lossy(b).into_owned(),
            Body::Empty => String::new(),
            Body::Stream(_) => "<stream>".into(),
        }
    }

    fn routes(list: &[(&str, &str, &'static str)]) -> Router {
        let mut r = Router::new();
        for &(method, pattern, name) in list {
            r.add(method, pattern, move |_, p| Response::text(200, format!("{name} {} {}", p.get("a").unwrap_or("-"), p.get("b").unwrap_or("-"))));
        }
        r
    }

    fn run(r: &Router, method: &str, path: &str) -> (u16, String) {
        let out = r.dispatch(&request(method, path, &[]));
        let detail = if out.status == 200 { text(&out) } else { out.header_value("allow").unwrap_or("").to_string() };
        (out.status, detail)
    }

    #[test]
    fn a_whole_path_must_match() {
        let r = routes(&[("GET", "/a/:a", "x")]);
        for path in ["/a", "/a/", "/a/1/2", "/a/1/", "/b/1", "/"] {
            check!(path, run(&r, "GET", path).0, 404);
        }
    }

    #[test]
    fn rest_takes_zero_or_more_segments() {
        let r = routes(&[("GET", "/s/*a", "rest")]);
        check!("/s/x/y.txt", run(&r, "GET", "/s/x/y.txt"), (200, "rest x/y.txt -".to_string()));
        check!("/s/", run(&r, "GET", "/s/"), (200, "rest  -".to_string()));
        check!("/s", run(&r, "GET", "/s"), (200, "rest  -".to_string()));
        check!("/sx", run(&r, "GET", "/sx").0, 404);
    }

    #[test]
    fn the_first_matching_route_wins() {
        let r = routes(&[("GET", "/a/new", "literal"), ("GET", "/a/:a", "param")]);
        check!("/a/new", run(&r, "GET", "/a/new"), (200, "literal - -".to_string()));
        let r = routes(&[("GET", "/a/:a", "param"), ("GET", "/a/new", "literal")]);
        check!("/a/new, the other order", run(&r, "GET", "/a/new"), (200, "param new -".to_string()));
    }

    #[test]
    fn a_later_route_with_the_right_method_still_matches() {
        let r = routes(&[("GET", "/a/:a", "get"), ("POST", "/a/new", "post")]);
        check!("POST /a/new", run(&r, "POST", "/a/new"), (200, "post - -".to_string()));
    }

    #[test]
    fn allow_lists_each_method_once() {
        let r = routes(&[("GET", "/x/y", "1"), ("PUT", "/x/:a", "2"), ("GET", "/x/:b", "3")]);
        check!("DELETE /x/y", run(&r, "DELETE", "/x/y"), (405, "GET, PUT".to_string()));
    }

    #[test]
    fn head_is_served_by_get_unless_there_is_a_head_route() {
        let r = routes(&[("GET", "/h", "get")]);
        check!("HEAD with only GET", run(&r, "HEAD", "/h").0, 200);
        let r = routes(&[("GET", "/h", "get"), ("HEAD", "/h", "head")]);
        check!("HEAD with its own route", run(&r, "HEAD", "/h"), (200, "head - -".to_string()));
        let r = routes(&[("POST", "/h", "post")]);
        check!("HEAD with only POST", run(&r, "HEAD", "/h"), (405, "POST".to_string()));
    }

    #[test]
    fn the_first_step_added_is_the_outermost() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let step = |name: &'static str, log: Arc<Mutex<Vec<String>>>| {
            move |req: &Request, next: Next<'_>| {
                log.lock().unwrap().push(format!("{name} in"));
                let out = next(req);
                log.lock().unwrap().push(format!("{name} out {}", out.status));
                out.header("X-Seen", name)
            }
        };
        let app = App::new(routes(&[("GET", "/", "r")])).wrap(step("a", log.clone())).wrap(step("b", log.clone()));
        let out = app.handle(&request("GET", "/", &[]));
        check!("order of calls", log.lock().unwrap().clone(), ["a in", "b in", "b out 200", "a out 200"].map(String::from).to_vec());
        let seen: Vec<&str> = out.headers.iter().filter(|(k, _)| k == "X-Seen").map(|(_, v)| v.as_str()).collect();
        check!("headers added from the inside out", seen, vec!["b", "a"]);
    }

    #[test]
    fn a_step_may_answer_by_itself_change_the_request_or_go_on_twice() {
        let calls = Arc::new(Mutex::new(0));
        let c = calls.clone();
        let mut r = Router::new();
        r.add("GET", "/real", move |req, _| {
            *c.lock().unwrap() += 1;
            Response::text(200, req.header("x-added").unwrap_or("none").to_string())
        });
        let app = App::new(r)
            .wrap(|req: &Request, next: Next<'_>| if req.path == "/blocked" { Response::new(403) } else { next(req) })
            .wrap(|req: &Request, next: Next<'_>| {
                let mut changed = req.clone();
                changed.path = "/real".into();
                changed.headers.push(("x-added".into(), "yes".into()));
                next(&changed);
                next(&changed)
            });
        check!("short circuit", (app.handle(&request("GET", "/blocked", &[])).status, *calls.lock().unwrap()), (403, 0));
        let out = app.handle(&request("GET", "/other", &[]));
        check!("rewritten and retried", (out.status, text(&out), *calls.lock().unwrap()), (200, "yes".to_string(), 2));
    }

    #[test]
    fn steps_outside_the_catching_one_see_the_500_and_the_app_keeps_working() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let s = seen.clone();
        let mut r = Router::new();
        r.add("GET", "/boom", |_, _| panic!("secret detail 1234"));
        r.add("GET", "/ok", |_, _| Response::text(200, "ok"));
        let app = App::new(r)
            .wrap(move |req: &Request, next: Next<'_>| {
                let out = next(req);
                s.lock().unwrap().push(out.status);
                out
            })
            .wrap(catch_panics())
            .wrap(|req: &Request, next: Next<'_>| next(req));
        for _ in 0..3 {
            let out = app.handle(&request("GET", "/boom", &[]));
            check!("500 without the panic message", (out.status, text(&out).contains("1234")), (500, false));
            check!("still serving", app.handle(&request("GET", "/ok", &[])).status, 200);
        }
        check!("what the outer step saw", seen.lock().unwrap().clone(), vec![500, 200, 500, 200, 500, 200]);
    }

    #[test]
    fn a_panic_in_a_step_inside_is_caught_and_outside_is_not() {
        let boom = |req: &Request, next: Next<'_>| {
            if req.path == "/p" {
                panic!("step bug");
            }
            next(req)
        };
        let app = App::new(Router::new()).wrap(catch_panics()).wrap(boom);
        check!("inside", app.handle(&request("GET", "/p", &[])).status, 500);
        let app = App::new(Router::new()).wrap(boom).wrap(catch_panics());
        let outside = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| app.handle(&request("GET", "/p", &[])).status));
        check!("outside the catching step the panic goes through", outside.is_err(), true);
    }

    #[test]
    fn without_the_catching_step_a_panic_goes_through() {
        let mut r = Router::new();
        r.add("GET", "/boom", |_, _| panic!("x"));
        let app = App::new(r);
        let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| app.handle(&request("GET", "/boom", &[])).status));
        check!("no catch_panics", out.is_err(), true);
    }

    #[test]
    fn bad_request_ids_are_replaced_and_new_ones_differ() {
        let app = App::new(Router::new()).wrap(request_ids());
        let long = "a".repeat(65);
        let mut made = std::collections::HashSet::new();
        for bad in ["", "has space", "semi;colon", "new\r\nline", long.as_str(), "é"] {
            let out = app.handle(&request("GET", "/", &[("x-request-id", bad)]));
            let id = out.header_value("x-request-id").unwrap_or("").to_string();
            check!(format!("{bad:?} replaced"), id.starts_with("req-"), true);
            made.insert(id);
        }
        check!("six different ids", made.len(), 6);
        let ok = "b".repeat(64);
        check!("64 characters are fine", app.handle(&request("GET", "/", &[("x-request-id", &ok)])).header_value("x-request-id").map(str::to_string), Some(ok));
    }

    #[test]
    fn an_app_is_shared_between_threads() {
        let app = Arc::new(App::new(routes(&[("GET", "/n/:a", "n")])).wrap(request_ids()).wrap(catch_panics()));
        let handles: Vec<_> = (0..4)
            .map(|t| {
                let app = app.clone();
                std::thread::spawn(move || (0..50).map(|i| text(&app.handle(&request("GET", &format!("/n/{t}-{i}"), &[])))).collect::<Vec<_>>())
            })
            .collect();
        for (t, h) in handles.into_iter().enumerate() {
            check!(format!("thread {t}"), h.join().unwrap(), (0..50).map(|i| format!("n {t}-{i} -")).collect::<Vec<_>>());
        }
    }

    #[test]
    fn random_routes_against_a_model() {
        let mut rng = anneal_prelude::Rng::new(81005);
        let words = ["a", "b", ""];
        for _ in 0..300 {
            let mut table: Vec<(&str, Vec<&str>, String)> = Vec::new();
            let mut router = Router::new();
            for i in 0..rng.below(5) {
                let method = *rng.pick(&["GET", "PUT"]);
                let len = 1 + rng.below(3);
                let mut pattern: Vec<&str> = (0..len).map(|_| *rng.pick(&["a", "b", ":x"])).collect();
                if rng.below(4) == 0 {
                    pattern.push("*rest");
                }
                let name = format!("r{i}");
                let n = name.clone();
                router.add(method, &format!("/{}", pattern.join("/")), move |_, _| Response::text(200, n.clone()));
                table.push((method, pattern, name));
            }
            let method = *rng.pick(&["GET", "PUT", "POST"]);
            let len = 1 + rng.below(4);
            let segs: Vec<&str> = (0..len).map(|_| *rng.pick(&words)).collect();
            let path = format!("/{}", segs.join("/"));
            let fits = |pattern: &[&str]| {
                let rest = pattern.last() == Some(&"*rest");
                let fixed = &pattern[..pattern.len() - usize::from(rest)];
                if segs.len() < fixed.len() || (!rest && segs.len() != fixed.len()) {
                    return false;
                }
                fixed.iter().zip(&segs).all(|(p, s)| if *p == ":x" { !s.is_empty() } else { p == s })
            };
            let hit = table.iter().find(|(m, p, _)| *m == method && fits(p));
            let want = match hit {
                Some((_, _, name)) => (200, name.clone()),
                None => {
                    let mut allow: Vec<&str> = Vec::new();
                    for (m, p, _) in &table {
                        if fits(p) && !allow.contains(m) {
                            allow.push(m);
                        }
                    }
                    if allow.is_empty() { (404, String::new()) } else { (405, allow.join(", ")) }
                }
            };
            let described: Vec<String> = table.iter().map(|(m, p, n)| format!("{m} /{} = {n}", p.join("/"))).collect();
            check!(format!("{method} {path} with {described:?}"), run(&router, method, &path), want);
        }
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
}

mod step06 {
    use super::*;

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
}

mod step07 {
    use super::*;

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
    fn hostile_paths_never_leak() {
        let site = Site::new();
        for raw in ["..", "../secret.txt", "sub/../../secret.txt", "sub/../index.txt", "%2e%2e/secret.txt", "..%2fsecret.txt", "%2E%2E%2Fsecret.txt", "%2e%2e%2f%2e%2e%2fetc/passwd", "sub%5c..%5c..%5csecret.txt", "sub\\x", "index.txt%00.png", "escape.txt", "neighbour/key.txt", "%65scape.txt"] {
            let r = site.get(raw, &[]);
            check!(raw, (r.status, String::from_utf8_lossy(&data(&r)).contains("SECRET")), (403, false));
        }
    }

    #[test]
    fn absolute_looking_paths_stay_inside() {
        let site = Site::new();
        check!("/etc/passwd", site.get("/etc/passwd", &[]).status, 404);
        check!("%2fetc%2fpasswd", site.get("%2fetc%2fpasswd", &[]).status, 404);
        check!("doubled and dot segments", data(&site.get(".//sub/./nested.json", &[])), b"{}".to_vec());
    }

    #[test]
    fn decoding_happens_once_and_only_for_good_escapes() {
        let site = Site::new();
        check!("space", site.get("with%20space.bin", &[]).status, 200);
        check!("twice encoded is a literal name", site.get("%252e%252e/secret.txt", &[]).status, 404);
        for raw in ["%", "%2", "%zz", "a%+1", "%ff%fe"] {
            check!(raw, site.get(raw, &[]).status, 400);
        }
    }

    #[test]
    fn missing_files_and_folders_are_404() {
        let site = Site::new();
        for raw in ["nope.txt", "", "sub", "sub/", "index.txt/x"] {
            check!(raw, site.get(raw, &[]).status, 404);
        }
    }

    #[test]
    fn a_link_that_stays_inside_is_served_and_a_linked_root_works() {
        let site = Site::new();
        check!("alias.txt", data(&site.get("alias.txt", &[])), b"hello from a file\n".to_vec());
        let linked = site.base.join("linked-root");
        std::os::unix::fs::symlink(&site.root, &linked).unwrap();
        check!("root reached through a link", serve_static(&linked, "index.txt", &request("GET", "/", &[])).status, 200);
    }

    #[test]
    fn content_types() {
        let site = Site::new();
        let types: Vec<String> = ["page.html", "sub/nested.json", "big.bin"].iter().map(|f| header(&site.get(f, &[]), "content-type")).collect();
        check!("html, json, other", types, ["text/html; charset=utf-8", "application/json", "application/octet-stream"].map(String::from).to_vec());
    }

    #[test]
    fn the_tag_changes_with_the_content() {
        let site = Site::new();
        let before = header(&site.get("index.txt", &[]), "etag");
        check!("same file, same tag", header(&site.get("index.txt", &[]), "etag"), before.clone());
        std::fs::write(site.root.join("index.txt"), "different and longer content\n").unwrap();
        let r = site.get("index.txt", &[("If-None-Match", &before)]);
        check!("old tag after a change", (r.status, header(&r, "etag") != before), (200, true));
    }

    #[test]
    fn tag_lists_weak_tags_and_the_star() {
        let site = Site::new();
        let tag = header(&site.get("index.txt", &[]), "etag");
        for value in [format!("\"a\", {tag}"), format!("W/{tag}"), "*".to_string()] {
            let r = site.get("index.txt", &[("If-None-Match", &value)]);
            check!(format!("If-None-Match: {value}"), (r.status, header(&r, "etag")), (304, tag.clone()));
        }
        check!("another tag", site.get("index.txt", &[("If-None-Match", "\"zz\"")]).status, 200);
    }

    #[test]
    fn the_date_is_used_only_without_a_tag() {
        let site = Site::new();
        let date = header(&site.get("index.txt", &[]), "last-modified");
        check!("same date", site.get("index.txt", &[("If-Modified-Since", &date)]).status, 304);
        check!("other date", site.get("index.txt", &[("If-Modified-Since", "Thu, 01 Jan 1970 00:00:00 GMT")]).status, 200);
        check!("a tag that does not match wins over a date that does", site.get("index.txt", &[("If-None-Match", "\"zz\""), ("If-Modified-Since", &date)]).status, 200);
        let meta = std::fs::metadata(site.root.join("index.txt")).unwrap();
        check!("the date is the file's time", date, http_date(meta.modified().unwrap()));
    }

    #[test]
    fn unchanged_wins_over_a_range() {
        let site = Site::new();
        let tag = header(&site.get("big.bin", &[]), "etag");
        check!("304", site.get("big.bin", &[("If-None-Match", &tag), ("Range", "bytes=0-1")]).status, 304);
    }

    #[test]
    fn ranges_open_suffix_clamped_and_unsatisfiable() {
        let site = Site::new();
        let big = Site::big();
        let case = |h: &str| {
            let r = site.get("big.bin", &[("Range", h)]);
            (r.status, header(&r, "content-range"), if r.status == 416 { Vec::new() } else { data(&r) })
        };
        check!("990-", case("bytes=990-"), (206, "bytes 990-999/1000".to_string(), big[990..].to_vec()));
        check!("-10", case("bytes=-10"), (206, "bytes 990-999/1000".to_string(), big[990..].to_vec()));
        check!("-5000", case("bytes=-5000"), (206, "bytes 0-999/1000".to_string(), big.clone()));
        check!("500-5000", case("bytes=500-5000"), (206, "bytes 500-999/1000".to_string(), big[500..].to_vec()));
        check!("huge end", case("bytes=0-99999999999999999999999"), (206, "bytes 0-999/1000".to_string(), big.clone()));
        for h in ["bytes=1000-", "bytes=2000-3000", "bytes=-0", "bytes=99999999999999999999999-"] {
            check!(h, case(h), (416, "bytes */1000".to_string(), Vec::new()));
        }
        for h in ["bytes=5-2", "bytes=a-b", "bytes=1-2,4-5", "items=0-1", "bytes=", "bytes=-", "bytes=+1-2"] {
            check!(h, case(h), (200, String::new(), big.clone()));
        }
    }

    #[test]
    fn an_empty_file() {
        let site = Site::new();
        check!("whole", (site.get("empty.bin", &[]).status, data(&site.get("empty.bin", &[])).len()), (200, 0));
        for h in ["bytes=0-", "bytes=-5"] {
            check!(h, site.get("empty.bin", &[("Range", h)]).status, 416);
        }
    }

    #[test]
    fn a_range_applies_only_to_get() {
        let site = Site::new();
        let r = serve_static(&site.root, "big.bin", &request("HEAD", "/", &[("Range", "bytes=0-9")]));
        check!("HEAD with a Range", (r.status, header(&r, "content-range")), (200, String::new()));
    }

    #[test]
    fn random_ranges_against_a_model() {
        let site = Site::new();
        let big = Site::big();
        let mut rng = anneal_prelude::Rng::new(81008);
        for _ in 0..300 {
            let a = rng.below(1200);
            let b = rng.below(1200);
            let (h, wanted): (String, Option<Vec<usize>>) = match rng.below(3) {
                0 => (format!("bytes={a}-{b}"), if b < a { None } else { Some((0..1000).filter(|i| *i >= a && *i <= b).collect()) }),
                1 => (format!("bytes={a}-"), Some((0..1000).filter(|i| *i >= a).collect())),
                _ => (format!("bytes=-{a}"), Some((0..1000).filter(|i| 1000 - *i <= a).collect())),
            };
            let r = site.get("big.bin", &[("Range", &h)]);
            match wanted {
                None => check!(h.clone(), (r.status, data(&r).len()), (200, 1000)),
                Some(w) if w.is_empty() => check!(h.clone(), r.status, 416),
                Some(w) => {
                    let bytes: Vec<u8> = w.iter().map(|&i| big[i]).collect();
                    check!(h.clone(), (r.status, data(&r), header(&r, "content-range")), (206, bytes, format!("bytes {}-{}/1000", w[0], w[w.len() - 1])));
                }
            }
        }
    }
}

mod step09 {
    use super::*;

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
}

mod step10 {
    use super::*;

    use std::sync::Arc;
    use std::time::{Duration, Instant};

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
    fn queued_connections_are_served_before_it_ends() {
        let gate = Gate::default();
        let server = start(Config { workers: 1, queue_cap: 4, ..Config::default() }, gate.clone(), Arc::new(()));
        let addr = server.addr();
        let mut busy = Client::connect(addr);
        busy.send(b"GET /block HTTP/1.1\r\nHost: t\r\n\r\n");
        gate.wait_for(1);
        let waiting: Vec<Client> = (0..3)
            .map(|_| {
                let mut c = Client::connect(addr);
                c.send(b"GET /hello HTTP/1.1\r\nHost: t\r\n\r\n");
                c
            })
            .collect();
        // a silent fourth fills the queue; the fifth is refused, which shows that those before it have been accepted
        let _silent = Client::connect(addr);
        check!("fifth connection", Client::connect(addr).response().status, 503);
        let finished = shut_down(server);
        wait_until_refused(addr);
        gate.open();
        check!("busy", busy.response().text(), "released".to_string());
        for (i, mut c) in waiting.into_iter().enumerate() {
            let w = c.response();
            check!(format!("queued {i}"), (w.text(), w.header("connection").map(str::to_string)), ("hello".to_string(), Some("close".to_string())));
        }
        check!("shutdown returns", finished.recv_timeout(Duration::from_secs(3)).is_ok(), true);
    }

    #[test]
    fn it_does_not_return_while_a_request_is_still_inside() {
        let gate = Gate::default();
        let server = start(Config::default(), gate.clone(), Arc::new(()));
        let addr = server.addr();
        let mut c = Client::connect(addr);
        c.send(b"GET /block HTTP/1.1\r\nHost: t\r\n\r\n");
        gate.wait_for(1);
        let finished = shut_down(server);
        wait_until_refused(addr);
        check!("still waiting for the handler", finished.recv_timeout(Duration::from_millis(150)).is_err(), true);
        gate.open();
        check!("then returns", finished.recv_timeout(Duration::from_secs(3)).is_ok(), true);
        check!("with the reply delivered", c.response().status, 200);
    }

    #[test]
    fn a_dripping_client_cannot_hold_it_open() {
        let config = Config { request_timeout: Duration::from_millis(400), idle_timeout: Duration::from_secs(60), ..Config::default() };
        let server = start(config, Gate::default(), Arc::new(()));
        let mut c = Client::connect(server.addr());
        c.send(b"GET /hello HTTP/1.1\r\n");
        let finished = shut_down(server);
        let mut returned = false;
        for _ in 0..100 {
            let _ = c.try_send(b"a");
            if finished.recv_timeout(Duration::from_millis(25)).is_ok() {
                returned = true;
                break;
            }
        }
        check!("returned while bytes were still arriving", returned, true);
    }

    #[test]
    fn a_request_half_sent_when_shutdown_starts_can_still_finish() {
        let server = start(Config::default(), Gate::default(), Arc::new(()));
        let addr = server.addr();
        let mut c = Client::connect(addr);
        c.send(b"GET /hello HTTP/1.1\r\nHost: t\r\n\r\n");
        c.response();
        c.send(b"GET /hello HTTP/1.1\r\n");
        // give the server time to see the first part before it is told to stop
        std::thread::sleep(Duration::from_millis(100));
        let finished = shut_down(server);
        wait_until_refused(addr);
        c.send(b"Host: t\r\n\r\n");
        let w = c.response();
        check!("finished", (w.status, w.header("connection").map(str::to_string)), (200, Some("close".to_string())));
        check!("returns", finished.recv_timeout(Duration::from_secs(3)).is_ok(), true);
    }

    #[test]
    fn many_idle_connections_and_waiting_ones() {
        let alive = Arc::new(());
        let server = start(Config { workers: 4, idle_timeout: Duration::from_secs(60), ..Config::default() }, Gate::default(), alive.clone());
        let clients: Vec<Client> = (0..9).map(|_| Client::connect(server.addr())).collect();
        check!("returns", shut_down(server).recv_timeout(Duration::from_secs(3)).is_ok(), true);
        check!("released", Arc::strong_count(&alive), 1);
        drop(clients);
    }

    #[test]
    fn a_server_nobody_used() {
        for _ in 0..10 {
            let alive = Arc::new(());
            let server = start(Config { workers: 2, ..Config::default() }, Gate::default(), alive.clone());
            check!("returns", shut_down(server).recv_timeout(Duration::from_secs(3)).is_ok(), true);
            check!("released", Arc::strong_count(&alive), 1);
        }
    }

    #[test]
    fn drop_waits_for_the_request_inside() {
        let gate = Gate::default();
        let alive = Arc::new(());
        let server = start(Config::default(), gate.clone(), alive.clone());
        let mut c = Client::connect(server.addr());
        c.send(b"GET /block HTTP/1.1\r\nHost: t\r\n\r\n");
        gate.wait_for(1);
        let (done, finished) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            drop(server);
            let _ = done.send(());
        });
        check!("drop is waiting", finished.recv_timeout(Duration::from_millis(150)).is_err(), true);
        gate.open();
        check!("drop returns", finished.recv_timeout(Duration::from_secs(3)).is_ok(), true);
        check!("reply", c.response().status, 200);
        check!("released", Arc::strong_count(&alive), 1);
    }

    #[test]
    fn one_server_stopping_leaves_another_running() {
        let a = start(Config::default(), Gate::default(), Arc::new(()));
        let b = start(Config::default(), Gate::default(), Arc::new(()));
        check!("a stops", shut_down(a).recv_timeout(Duration::from_secs(3)).is_ok(), true);
        check!("b serves", fetch(b.addr(), "/hello").status, 200);
    }
}
