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
