use solution::*;

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
