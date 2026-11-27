//! RECIPE 155 (protocols): RESP2, the Redis wire protocol: an incremental zero-copy parser, serializer, limits and a mini server core.
//! LEVEL: advanced
//! Use in: any network protocol over TCP (framing, pipelining, back-pressure), Redis-compatible servers/clients, fuzz-safe parsers.   Run: cargo run --release --example resp_protocol
#![allow(dead_code)]
//!
//! WHAT: RESP2 has five frame kinds, each starting with a type byte and ending in CRLF: `+simple`, `-error`, `:integer`,
//! `$len\r\nbytes\r\n` (bulk; `$-1` is null) and `*n\r\n<n frames>` (array; `*-1` is null). Clients send commands as arrays of
//! bulk strings, or as a bare "inline" line (`PING\r\n`) for telnet debugging.
//! WHY INCREMENTAL: TCP delivers arbitrary byte chunks, so a frame may be split anywhere. `parse(&[u8])` returns
//! `Ok(None)` ("need more bytes"), `Ok(Some((frame, consumed)))`, or `Err` (the stream is poisoned; reply and close).
//! The caller keeps a buffer, appends reads, and loops `parse` + drain: that also gives PIPELINING for free.
//! (Re-parsing from the start on every read is O(n^2) for one huge frame; real servers keep resumable parser state.)
//! ZERO-COPY: bulk/simple/error payloads are `Cow::Borrowed` slices of the input; only replies we build are `Owned`.
//! HOSTILE INPUT: every length is untrusted. `$99999999999` must not allocate, `*1\r\n` repeated 100k times must not
//! recurse 100k deep, and a line with no CRLF must not buffer forever. `Limits` caps line, bulk, array, depth and the whole frame
//! (many elements that are each within limits must not add up without end), and we never pre-allocate from a declared count
//! (capacity is `min(n, 64)`). The caller must still read in modest chunks and call `serve` after each. REPLIES are hostile too:
//! an error message that quotes the client's bytes could contain CRLF and forge further replies, so the encoder turns CR and LF in
//! simple strings and errors into spaces.
//! LIMITS: the whole-frame limit does not bound the quadratic re-parsing by itself (a frame of many tiny elements arriving in tiny
//! pieces is still expensive), and the store has no memory limit.
//! COMMON USES: Redis/Valkey clients and servers, protocol proxies and load balancers (e.g. connection-pool middleware),
//! test doubles for Redis, and the same length-prefixed "need more bytes" parser shape in HTTP/1, Postgres and Kafka framing.
//! Read: https://redis.io/docs/latest/develop/reference/protocol-spec/
use std::borrow::Cow;
use std::cell::Cell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

// ---------------------------------------------------------------- frames

#[derive(Debug, Clone, PartialEq)]
enum Frame<'a> {
    Simple(Cow<'a, str>),
    Error(Cow<'a, str>),
    Integer(i64),
    Bulk(Cow<'a, [u8]>),
    Null,
    Array(Vec<Frame<'a>>),
    NullArray,
}

impl Frame<'static> {
    fn simple(s: &str) -> Self {
        Frame::Simple(Cow::Owned(s.to_string()))
    }
    fn err(s: impl Into<String>) -> Self {
        Frame::Error(Cow::Owned(s.into()))
    }
    fn bulk(b: &[u8]) -> Self {
        Frame::Bulk(Cow::Owned(b.to_vec()))
    }
}

impl<'a> Frame<'a> {
    /// detach from the input buffer (needed before the buffer is drained or reused)
    fn into_owned(self) -> Frame<'static> {
        match self {
            Frame::Simple(s) => Frame::Simple(Cow::Owned(s.into_owned())),
            Frame::Error(s) => Frame::Error(Cow::Owned(s.into_owned())),
            Frame::Bulk(b) => Frame::Bulk(Cow::Owned(b.into_owned())),
            Frame::Array(v) => Frame::Array(v.into_iter().map(Frame::into_owned).collect()),
            Frame::Integer(n) => Frame::Integer(n),
            Frame::Null => Frame::Null,
            Frame::NullArray => Frame::NullArray,
        }
    }
    /// true if no payload was copied out of the input
    fn is_borrowed(&self) -> bool {
        match self {
            Frame::Simple(s) | Frame::Error(s) => matches!(s, Cow::Borrowed(_)),
            Frame::Bulk(b) => matches!(b, Cow::Borrowed(_)),
            Frame::Array(v) => v.iter().all(Frame::is_borrowed),
            _ => true,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum ProtoError {
    BadInteger,
    BadLength,
    BadTerminator,
    InvalidUtf8,
    TooLarge(&'static str),
    TooDeep,
    LineTooLong,
    InlineInArray,
}

impl fmt::Display for ProtoError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ProtoError::TooLarge(what) => write!(f, "{what} exceeds the configured limit"),
            other => write!(f, "{other:?}"),
        }
    }
}

#[derive(Debug, Clone)]
struct Limits {
    max_line: usize,
    max_bulk: usize,
    max_array: usize,
    max_depth: usize,
    /// bytes in one top-level frame
    max_frame: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_line: 64 * 1024,
            max_bulk: 8 << 20,
            max_array: 1 << 20,
            max_depth: 32,
            max_frame: 16 << 20,
        }
    }
}

type Parsed<'a> = Result<Option<(Frame<'a>, usize)>, ProtoError>;

// ---------------------------------------------------------------- parser

fn parse(buf: &[u8]) -> Parsed<'_> {
    parse_with(buf, &Limits::default())
}

fn parse_with<'a>(buf: &'a [u8], lim: &Limits) -> Parsed<'a> {
    match parse_at(buf, 0, lim)? {
        Some((_, used)) if used > lim.max_frame => Err(ProtoError::TooLarge("frame")),
        None if buf.len() > lim.max_frame => Err(ProtoError::TooLarge("frame")),
        parsed => Ok(parsed),
    }
}

/// Find a CRLF-terminated line. The limit applies even when the CRLF has not arrived yet:
/// otherwise an attacker just never sends one and we buffer forever.
fn take_line<'a>(buf: &'a [u8], lim: &Limits) -> Result<Option<(&'a [u8], usize)>, ProtoError> {
    match buf.windows(2).position(|w| w == b"\r\n") {
        Some(i) if i > lim.max_line => Err(ProtoError::LineTooLong),
        Some(i) => Ok(Some((&buf[..i], i + 2))),
        // a trailing CR may be the first half of the terminator: it does not count
        None if buf.strip_suffix(b"\r").unwrap_or(buf).len() > lim.max_line => {
            Err(ProtoError::LineTooLong)
        }
        None => Ok(None),
    }
}

/// Strict decimal: Rust's `parse` accepts a leading '+', RESP does not.
fn parse_i64(b: &[u8]) -> Option<i64> {
    if b.first() == Some(&b'+') {
        return None;
    }
    std::str::from_utf8(b).ok()?.parse().ok()
}

fn utf8(b: &[u8]) -> Result<&str, ProtoError> {
    std::str::from_utf8(b).map_err(|_| ProtoError::InvalidUtf8)
}

fn parse_at<'a>(buf: &'a [u8], depth: usize, lim: &Limits) -> Parsed<'a> {
    let Some(&first) = buf.first() else {
        return Ok(None);
    };
    if !matches!(first, b'+' | b'-' | b':' | b'$' | b'*') {
        return parse_inline(buf, depth, lim);
    }
    let Some((line, used)) = take_line(buf, lim)? else {
        return Ok(None);
    };
    let body = &line[1..];
    match first {
        b'+' => Ok(Some((Frame::Simple(Cow::Borrowed(utf8(body)?)), used))),
        b'-' => Ok(Some((Frame::Error(Cow::Borrowed(utf8(body)?)), used))),
        b':' => Ok(Some((
            Frame::Integer(parse_i64(body).ok_or(ProtoError::BadInteger)?),
            used,
        ))),
        b'$' => parse_bulk(buf, body, used, lim),
        _ => parse_array(buf, body, used, depth, lim),
    }
}

fn parse_bulk<'a>(buf: &'a [u8], header: &[u8], used: usize, lim: &Limits) -> Parsed<'a> {
    let n = parse_i64(header).ok_or(ProtoError::BadInteger)?;
    if n == -1 {
        return Ok(Some((Frame::Null, used)));
    }
    let n = usize::try_from(n).map_err(|_| ProtoError::BadLength)?;
    if n > lim.max_bulk {
        return Err(ProtoError::TooLarge("bulk string")); // checked BEFORE waiting for the bytes
    }
    let end = used + n;
    if buf.len() < end + 2 {
        return Ok(None);
    }
    if &buf[end..end + 2] != b"\r\n" {
        return Err(ProtoError::BadTerminator);
    }
    Ok(Some((Frame::Bulk(Cow::Borrowed(&buf[used..end])), end + 2)))
}

fn parse_array<'a>(
    buf: &'a [u8],
    header: &[u8],
    used: usize,
    depth: usize,
    lim: &Limits,
) -> Parsed<'a> {
    let n = parse_i64(header).ok_or(ProtoError::BadInteger)?;
    if n == -1 {
        return Ok(Some((Frame::NullArray, used)));
    }
    let n = usize::try_from(n).map_err(|_| ProtoError::BadLength)?;
    if n > lim.max_array {
        return Err(ProtoError::TooLarge("array"));
    }
    if depth >= lim.max_depth {
        return Err(ProtoError::TooDeep); // bounds recursion, so the stack cannot overflow
    }
    let mut items = Vec::with_capacity(n.min(64)); // never trust the declared count for allocation
    let mut pos = used;
    for _ in 0..n {
        match parse_at(&buf[pos..], depth + 1, lim)? {
            Some((frame, c)) => {
                items.push(frame);
                pos += c;
            }
            None => return Ok(None),
        }
    }
    Ok(Some((Frame::Array(items), pos)))
}

/// Inline command: `SET a 1\n` becomes the array ["SET","a","1"]. Quoting is not supported here.
fn parse_inline<'a>(buf: &'a [u8], depth: usize, lim: &Limits) -> Parsed<'a> {
    if depth > 0 {
        return Err(ProtoError::InlineInArray);
    }
    let Some(nl) = buf.iter().position(|&b| b == b'\n') else {
        return if buf.strip_suffix(b"\r").unwrap_or(buf).len() > lim.max_line {
            Err(ProtoError::LineTooLong)
        } else {
            Ok(None)
        };
    };
    let line = buf[..nl].strip_suffix(b"\r").unwrap_or(&buf[..nl]);
    if line.len() > lim.max_line {
        return Err(ProtoError::LineTooLong); // also when the newline came in the same read
    }
    let words = line
        .split(|b| b.is_ascii_whitespace())
        .filter(|w| !w.is_empty())
        .map(|w| Frame::Bulk(Cow::Borrowed(w)))
        .collect();
    Ok(Some((Frame::Array(words), nl + 1)))
}

// ---------------------------------------------------------------- serializer

fn encode(frame: &Frame, out: &mut Vec<u8>) {
    let line = |out: &mut Vec<u8>, tag: u8, body: &[u8]| {
        out.push(tag);
        out.extend_from_slice(body);
        out.extend_from_slice(b"\r\n");
    };
    // these two end at the first CRLF, so their text must not contain one
    let one_line = |s: &str| s.replace(['\r', '\n'], " ").into_bytes();
    match frame {
        Frame::Simple(s) => line(out, b'+', &one_line(s)),
        Frame::Error(s) => line(out, b'-', &one_line(s)),
        Frame::Integer(n) => line(out, b':', n.to_string().as_bytes()),
        Frame::Null => line(out, b'$', b"-1"),
        Frame::NullArray => line(out, b'*', b"-1"),
        Frame::Bulk(b) => {
            line(out, b'$', b.len().to_string().as_bytes());
            out.extend_from_slice(b);
            out.extend_from_slice(b"\r\n");
        }
        Frame::Array(items) => {
            line(out, b'*', items.len().to_string().as_bytes());
            items.iter().for_each(|f| encode(f, out));
        }
    }
}

fn encode_command(words: &[&str]) -> Vec<u8> {
    let frame = Frame::Array(words.iter().map(|w| Frame::bulk(w.as_bytes())).collect());
    let mut out = Vec::new();
    encode(&frame, &mut out);
    out
}

// ---------------------------------------------------------------- command executor

/// Injectable time source so TTL logic is testable without sleeping.
trait Clock {
    fn now_ms(&self) -> u64;
}

struct FakeClock(Cell<u64>);
impl FakeClock {
    fn advance(&self, ms: u64) {
        self.0.set(self.0.get() + ms);
    }
}
impl Clock for Rc<FakeClock> {
    fn now_ms(&self) -> u64 {
        self.0.get()
    }
}

struct Entry {
    value: Vec<u8>,
    expires_at: Option<u64>,
}

struct Store<C: Clock> {
    map: HashMap<Vec<u8>, Entry>,
    clock: C,
}

const NOT_INT: &str = "ERR value is not an integer or out of range";
const KNOWN: [&str; 9] = [
    "PING", "ECHO", "SET", "GET", "DEL", "INCR", "EXISTS", "EXPIRE", "TTL",
];

impl<C: Clock> Store<C> {
    fn new(clock: C) -> Self {
        Store {
            map: HashMap::new(),
            clock,
        }
    }

    /// Lazy expiry: an expired key is deleted when it is touched, so no timer thread is needed.
    fn live(&mut self, key: &[u8]) -> Option<&mut Entry> {
        let now = self.clock.now_ms();
        if self
            .map
            .get(key)
            .is_some_and(|e| e.expires_at.is_some_and(|t| t <= now))
        {
            self.map.remove(key);
        }
        self.map.get_mut(key)
    }

    fn execute(&mut self, cmd: &Frame) -> Frame<'static> {
        let Some(args) = command_args(cmd) else {
            return Frame::err("ERR Protocol error: expected array of bulk strings");
        };
        let Some((name, rest)) = args.split_first() else {
            return Frame::err("ERR empty command");
        };
        let name = name.to_ascii_uppercase();
        match (name.as_slice(), rest) {
            (b"PING", []) => Frame::simple("PONG"),
            (b"PING", [msg]) | (b"ECHO", [msg]) => Frame::bulk(msg),
            (b"SET", [k, v, opts @ ..]) => self.set(k, v, opts),
            (b"GET", [k]) => self.live(k).map_or(Frame::Null, |e| Frame::bulk(&e.value)),
            (b"DEL", keys) if !keys.is_empty() => {
                let now = self.clock.now_ms();
                let n = keys
                    .iter()
                    .filter(|k| {
                        self.map
                            .remove(**k)
                            .is_some_and(|e| e.expires_at.is_none_or(|t| t > now))
                    })
                    .count();
                Frame::Integer(n as i64)
            }
            (b"EXISTS", keys) if !keys.is_empty() => {
                Frame::Integer(keys.iter().filter(|k| self.live(k).is_some()).count() as i64)
            }
            (b"INCR", [k]) => self.incr(k),
            (b"EXPIRE", [k, secs]) => self.expire(k, secs),
            (b"TTL", [k]) => self.ttl(k),
            (b"KEYS", [pat]) => self.keys(pat),
            _ if name == b"KEYS" || KNOWN.iter().any(|k| k.as_bytes() == name) => {
                Frame::err(format!(
                    "ERR wrong number of arguments for '{}' command",
                    String::from_utf8_lossy(&name).to_ascii_lowercase()
                ))
            }
            _ => Frame::err(format!(
                "ERR unknown command '{}'",
                String::from_utf8_lossy(&name)
            )),
        }
    }

    fn set(&mut self, k: &[u8], v: &[u8], opts: &[&[u8]]) -> Frame<'static> {
        let (mut nx, mut ttl_ms, mut i) = (false, None, 0);
        while i < opts.len() {
            match opts[i].to_ascii_uppercase().as_slice() {
                b"NX" => nx = true,
                b"EX" => {
                    i += 1;
                    match opts.get(i).and_then(|b| parse_i64(b)) {
                        Some(s) if s > 0 => ttl_ms = Some((s as u64).saturating_mul(1000)),
                        _ => return Frame::err("ERR invalid expire time in 'set' command"),
                    }
                }
                _ => return Frame::err("ERR syntax error"),
            }
            i += 1;
        }
        if nx && self.live(k).is_some() {
            return Frame::Null;
        }
        let now = self.clock.now_ms();
        // a plain SET also clears any previous TTL, like Redis
        let expires_at = ttl_ms.map(|ms| now.saturating_add(ms));
        self.map.insert(
            k.to_vec(),
            Entry {
                value: v.to_vec(),
                expires_at,
            },
        );
        Frame::simple("OK")
    }

    fn incr(&mut self, k: &[u8]) -> Frame<'static> {
        self.live(k);
        let e = self.map.entry(k.to_vec()).or_insert_with(|| Entry {
            value: b"0".to_vec(),
            expires_at: None,
        });
        match parse_i64(&e.value).and_then(|n| n.checked_add(1)) {
            Some(n) => {
                e.value = n.to_string().into_bytes();
                Frame::Integer(n)
            }
            None => Frame::err(NOT_INT),
        }
    }

    fn expire(&mut self, k: &[u8], secs: &[u8]) -> Frame<'static> {
        let Some(s) = parse_i64(secs) else {
            return Frame::err(NOT_INT);
        };
        let now = self.clock.now_ms();
        if s <= 0 {
            let existed = self.live(k).is_some(); // an expired key no longer exists
            self.map.remove(k);
            return Frame::Integer(existed as i64);
        }
        match self.live(k) {
            Some(e) => {
                e.expires_at = Some(now.saturating_add((s as u64).saturating_mul(1000)));
                Frame::Integer(1)
            }
            None => Frame::Integer(0),
        }
    }

    fn ttl(&mut self, k: &[u8]) -> Frame<'static> {
        let now = self.clock.now_ms();
        Frame::Integer(match self.live(k) {
            None => -2,
            Some(Entry {
                expires_at: None, ..
            }) => -1,
            Some(Entry {
                expires_at: Some(t),
                ..
            }) => ((*t - now).saturating_add(500) / 1000) as i64, // rounds like Redis
        })
    }

    fn keys(&mut self, pat: &[u8]) -> Frame<'static> {
        let now = self.clock.now_ms();
        let mut found: Vec<&Vec<u8>> = self
            .map
            .iter()
            .filter(|(k, e)| e.expires_at.is_none_or(|t| t > now) && glob(pat, k))
            .map(|(k, _)| k)
            .collect();
        found.sort(); // HashMap order is random; sort so replies are deterministic
        Frame::Array(found.into_iter().map(|k| Frame::bulk(k)).collect())
    }
}

/// A command must be an array of bulk strings.
fn command_args<'f>(frame: &'f Frame) -> Option<Vec<&'f [u8]>> {
    let Frame::Array(items) = frame else {
        return None;
    };
    items
        .iter()
        .map(|f| {
            if let Frame::Bulk(b) = f {
                Some(b.as_ref())
            } else {
                None
            }
        })
        .collect()
}

/// Glob with only `*`. Iterative two-pointer with one backtrack point: O(len(pat) * len(s)) worst case, no recursion.
fn glob(pat: &[u8], s: &[u8]) -> bool {
    let (mut p, mut i, mut star) = (0, 0, None::<(usize, usize)>);
    while i < s.len() {
        if p < pat.len() && pat[p] == b'*' {
            star = Some((p, i));
            p += 1;
        } else if p < pat.len() && pat[p] == s[i] {
            p += 1;
            i += 1;
        } else if let Some((sp, si)) = star {
            (p, i, star) = (sp + 1, si + 1, Some((sp, si + 1)));
        } else {
            return false;
        }
    }
    pat[p..].iter().all(|&c| c == b'*')
}

/// Serve every complete command currently buffered; leave a partial one in `input`.
fn serve<C: Clock>(
    store: &mut Store<C>,
    input: &mut Vec<u8>,
    out: &mut Vec<u8>,
) -> Result<(), ProtoError> {
    loop {
        let (reply, used) = match parse(input) {
            Ok(None) => return Ok(()),
            Ok(Some((Frame::Array(a), used))) if a.is_empty() => (None, used), // blank inline line
            Ok(Some((frame, used))) => (Some(store.execute(&frame)), used),
            Err(e) => {
                encode(&Frame::err(format!("ERR Protocol error: {e}")), out);
                input.clear();
                return Err(e);
            }
        };
        if let Some(r) = reply {
            encode(&r, out);
        }
        input.drain(..used);
    }
}

// ---------------------------------------------------------------- helpers: pretty printer, rng

/// redis-cli style rendering for transcripts.
fn pretty(f: &Frame) -> String {
    match f {
        Frame::Simple(s) => s.to_string(),
        Frame::Error(e) => format!("(error) {e}"),
        Frame::Integer(n) => format!("(integer) {n}"),
        Frame::Bulk(b) => format!("{:?}", String::from_utf8_lossy(b)),
        Frame::Null | Frame::NullArray => "(nil)".into(),
        Frame::Array(a) if a.is_empty() => "(empty array)".into(),
        Frame::Array(a) => a
            .iter()
            .enumerate()
            .map(|(i, x)| format!("{}) {}", i + 1, pretty(x).replace('\n', "\n   ")))
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

fn gen_frame(rng: &mut Rng, depth: usize) -> Frame<'static> {
    let kinds = if depth < 4 { 7 } else { 5 };
    match rng.below(kinds) {
        0 => Frame::simple(
            &(0..rng.below(8))
                .map(|_| (b'a' + rng.below(26) as u8) as char)
                .collect::<String>(),
        ),
        1 => Frame::err(format!("ERR e{}", rng.below(100))),
        2 => Frame::Integer(rng.next() as i64),
        3 => Frame::Bulk(Cow::Owned(
            (0..rng.below(24))
                .map(|_| *b"\r\n$*x\0\xff".get(rng.below(9)).unwrap_or(&b'a'))
                .collect(),
        )),
        4 => Frame::Null,
        5 => Frame::Array(
            (0..rng.below(5))
                .map(|_| gen_frame(rng, depth + 1))
                .collect(),
        ),
        _ => Frame::NullArray,
    }
}

/// Parse until the buffer is exhausted or incomplete; every step must make progress (no infinite loop).
fn drain_all(mut buf: &[u8], lim: &Limits) -> Result<usize, ProtoError> {
    let mut n = 0;
    while !buf.is_empty() {
        match parse_with(buf, lim)? {
            None => break,
            Some((_, used)) => {
                assert!(
                    used > 0 && used <= buf.len(),
                    "parser must consume 1..=len bytes"
                );
                buf = &buf[used..];
                n += 1;
            }
        }
    }
    Ok(n)
}

// ---------------------------------------------------------------- self-checks

fn test_round_trip_and_splits() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut stream = Vec::new();
    let mut originals = Vec::new();
    for _ in 0..2000 {
        let f = gen_frame(&mut rng, 0);
        let mut enc = Vec::new();
        encode(&f, &mut enc);
        let (back, used) = parse(&enc).unwrap().expect("complete frame");
        assert_eq!((&back, used), (&f, enc.len()));
        assert!(back.is_borrowed(), "parser copied payload bytes");
        if originals.len() < 60 {
            // EVERY strict prefix of a valid frame must be "need more", never an error and never a frame
            for cut in 0..enc.len() {
                assert_eq!(parse(&enc[..cut]), Ok(None), "prefix {cut} of {enc:?}");
            }
            stream.extend_from_slice(&enc);
            originals.push(f);
        }
    }
    // pipelining: all frames in one buffer
    let mut buf = &stream[..];
    for want in &originals {
        let (f, used) = parse(buf).unwrap().unwrap();
        assert_eq!(&f, want);
        buf = &buf[used..];
    }
    assert!(buf.is_empty());
    // byte-by-byte AND random-sized chunks must give identical results
    for chunk_mode in [0usize, 1] {
        let (mut pending, mut got, mut at) = (Vec::new(), Vec::new(), 0);
        while at < stream.len() {
            let step = if chunk_mode == 0 { 1 } else { 1 + rng.below(9) };
            pending.extend_from_slice(&stream[at..(at + step).min(stream.len())]);
            at += step;
            while let Some((f, used)) = parse(&pending).unwrap() {
                got.push(f.into_owned());
                pending.drain(..used);
            }
        }
        assert!(pending.is_empty());
        assert_eq!(got, originals);
    }
}

fn test_limits_and_malformed() {
    assert_eq!(
        parse(b"$99999999999\r\n"),
        Err(ProtoError::TooLarge("bulk string"))
    );
    assert_eq!(
        parse(b"*99999999999\r\n"),
        Err(ProtoError::TooLarge("array"))
    );
    assert_eq!(parse(b"$-5\r\n"), Err(ProtoError::BadLength));
    assert_eq!(parse(b":12x\r\n"), Err(ProtoError::BadInteger));
    assert_eq!(parse(b":+1\r\n"), Err(ProtoError::BadInteger));
    assert_eq!(parse(b"$3\r\nabcXY"), Err(ProtoError::BadTerminator));
    assert_eq!(parse(b"*1\r\nPING\r\n"), Err(ProtoError::InlineInArray));
    assert_eq!(parse(b"+\xff\r\n"), Err(ProtoError::InvalidUtf8));
    assert_eq!(parse(b"*1048576\r\n"), Ok(None)); // within limits: just waits for data, allocates ~nothing
    let deep = b"*1\r\n".repeat(100_000);
    assert_eq!(parse(&deep), Err(ProtoError::TooDeep));
    assert_eq!(parse(&vec![b'a'; 70_000]), Err(ProtoError::LineTooLong));
    assert_eq!(
        parse(&[b"+".as_slice(), &vec![b'a'; 70_000]].concat()),
        Err(ProtoError::LineTooLong)
    );
    // inline commands, with and without CR
    let (f, used) = parse(b"SET  a   1\r\nGET a\r\n").unwrap().unwrap();
    assert_eq!(
        (pretty(&f), used),
        ("1) \"SET\"\n2) \"a\"\n3) \"1\"".to_string(), 12)
    );
    assert_eq!(parse(b"PING\n").unwrap().unwrap().1, 5);
}

fn test_hostile() {
    // client bytes quoted in an error must not forge a second reply
    let mut store = Store::new(Rc::new(FakeClock(Cell::new(0))));
    let mut out = Vec::new();
    serve(&mut store, &mut encode_command(&["x\r\n+OK"]), &mut out).unwrap();
    let (reply, used) = parse(&out).unwrap().unwrap();
    assert!(
        matches!(reply, Frame::Error(_)) && used == out.len(),
        "one command, one reply: {out:?}"
    );
    out.clear();
    encode(&Frame::simple("a\nb"), &mut out);
    assert_eq!(out, b"+a b\r\n");

    // the line limit does not depend on how the bytes were split
    let long = [&vec![b'a'; 70_000][..], b"\n"].concat();
    assert_eq!(parse(&long), Err(ProtoError::LineTooLong));
    let max = Limits::default().max_line;
    for full in [
        [&b"+"[..], &vec![b'a'; max - 1], b"\r\n"].concat(),
        [&vec![b'a'; max][..], b"\r\n"].concat(),
    ] {
        assert!(matches!(parse(&full), Ok(Some(_))));
        assert_eq!(
            parse(&full[..full.len() - 1]),
            Ok(None),
            "CR arrived, LF not yet"
        );
    }

    // elements within every other limit must not add up for ever
    let small = Limits {
        max_frame: 64,
        ..Limits::default()
    };
    let mut frame = b"*100\r\n".to_vec();
    frame.extend(b"$4\r\nabcd\r\n".repeat(8));
    assert_eq!(
        parse_with(&frame, &small),
        Err(ProtoError::TooLarge("frame"))
    );
    let whole = [&b"$100\r\n"[..], &[b'a'; 100], b"\r\n"].concat();
    assert_eq!(
        parse_with(&whole, &small),
        Err(ProtoError::TooLarge("frame"))
    );
    let pipelined = b"+ok\r\n".repeat(100); // many small frames are fine
    assert_eq!(drain_all(&pipelined, &small), Ok(100));

    // expiry at the edges
    let clock = Rc::new(FakeClock(Cell::new(0)));
    let mut store = Store::new(clock.clone());
    let mut run = |words: &[&str]| {
        let mut out = Vec::new();
        serve(&mut store, &mut encode_command(words), &mut out).unwrap();
        String::from_utf8(out).unwrap()
    };
    run(&["SET", "k", "v", "EX", "1"]);
    clock.advance(1000);
    assert_eq!(run(&["EXPIRE", "k", "-1"]), ":0\r\n", "it was already gone");
    clock.0.set(0); // with the time below 500 ms, adding the rounding after the subtraction would overflow
    run(&["SET", "k", "v", "EX", "9223372036854775807"]);
    // the expiry saturates at u64::MAX ms; a sum that wrapped round would answer 0 in a release build
    assert_eq!(run(&["TTL", "k"]), format!(":{}\r\n", u64::MAX / 1000));
}

fn test_fuzz() {
    let mut rng = Rng(0xDEAD_BEEF_1234_5678);
    let lim = Limits {
        max_line: 256,
        max_bulk: 4096,
        max_array: 1024,
        max_depth: 8,
        max_frame: 8192,
    };
    let alphabet = b"*$+-:\r\n0123456789-abc ";
    let (mut errs, mut oks) = (0, 0);
    for round in 0..20_000 {
        let buf: Vec<u8> = if round % 2 == 0 {
            (0..rng.below(80))
                .map(|_| {
                    if rng.below(4) == 0 {
                        rng.next() as u8
                    } else {
                        alphabet[rng.below(alphabet.len())]
                    }
                })
                .collect()
        } else {
            // mutate a valid pipelined stream: flip, delete, insert, truncate
            let mut v = Vec::new();
            (0..3).for_each(|_| encode(&gen_frame(&mut rng, 0), &mut v));
            for _ in 0..1 + rng.below(4) {
                let at = rng.below(v.len().max(1));
                match rng.below(4) {
                    0 if at < v.len() => v[at] ^= 1 << rng.below(8),
                    1 if at < v.len() => drop(v.remove(at)),
                    2 => v.insert(at, alphabet[rng.below(alphabet.len())]),
                    _ => v.truncate(at),
                }
            }
            v
        };
        match drain_all(&buf, &lim) {
            Ok(_) => oks += 1,
            Err(_) => errs += 1,
        }
        // the full server path must also survive anything
        let mut store = Store::new(Rc::new(FakeClock(Cell::new(0))));
        let (mut input, mut out) = (buf.clone(), Vec::new());
        let _ = serve(&mut store, &mut input, &mut out);
    }
    assert!(
        errs > 1000 && oks > 1000,
        "fuzz should exercise both outcomes ({oks} ok, {errs} err)"
    );
    println!("fuzz: 20000 inputs, {oks} parsed cleanly, {errs} rejected, 0 panics");
}

fn test_glob() {
    assert!(
        glob(b"*", b"")
            && glob(b"a*c", b"abbc")
            && glob(b"*b*", b"abc")
            && glob(b"user:*:name", b"user:1:name")
    );
    assert!(!glob(b"a*c", b"abd") && !glob(b"abc", b"ab") && !glob(b"", b"x") && glob(b"", b""));
    assert!(!glob(
        &[b"a*".repeat(30), b"c".to_vec()].concat(),
        &[vec![b'a'; 60], b"b".to_vec()].concat()
    )); // pathological pattern stays fast
}

fn run_script() {
    let clock = Rc::new(FakeClock(Cell::new(1_000_000)));
    let mut store = Store::new(clock.clone());
    let script: &[(&str, &str)] = &[
        ("PING", "PONG"),
        ("SET greeting hello", "OK"),
        ("GET greeting", "\"hello\""),
        ("SET greeting x NX", "(nil)"),
        ("INCR counter", "(integer) 1"),
        ("INCR counter", "(integer) 2"),
        ("SET temp v EX 10", "OK"),
        ("TTL temp", "(integer) 10"),
        ("sleep 4000", ""),
        ("TTL temp", "(integer) 6"),
        ("sleep 6000", ""),
        ("GET temp", "(nil)"),
        ("TTL temp", "(integer) -2"),
        ("TTL greeting", "(integer) -1"),
        ("KEYS *", "1) \"counter\"\n2) \"greeting\""),
        ("KEYS g*g", "1) \"greeting\""),
        ("EXISTS greeting counter nope", "(integer) 2"),
        ("EXPIRE counter 100", "(integer) 1"),
        ("DEL greeting nope", "(integer) 1"),
        ("SET counter abc", "OK"),
        (
            "INCR counter",
            "(error) ERR value is not an integer or out of range",
        ),
        ("ECHO hi", "\"hi\""),
        ("FOO", "(error) ERR unknown command 'FOO'"),
        (
            "GET",
            "(error) ERR wrong number of arguments for 'get' command",
        ),
    ];
    println!("--- scripted session (fake clock) ---");
    for (cmd, want) in script {
        if let Some(ms) = cmd.strip_prefix("sleep ") {
            clock.advance(ms.parse().unwrap());
            println!("   ... {ms} ms pass");
            continue;
        }
        let words: Vec<&str> = cmd.split_whitespace().collect();
        let (mut input, mut out) = (encode_command(&words), Vec::new());
        serve(&mut store, &mut input, &mut out).unwrap();
        let (reply, used) = parse(&out).unwrap().unwrap();
        assert_eq!(used, out.len());
        let shown = pretty(&reply);
        println!("> {cmd}\n{shown}");
        assert_eq!(&shown, want, "command {cmd}");
    }
    // pipelined + inline + partial tail, in one buffer
    let mut input =
        b"PING\r\nSET k 5\r\n*2\r\n$4\r\nINCR\r\n$1\r\nk\r\n\r\nGET k\r\n*2\r\n$3\r\nGE".to_vec();
    let mut out = Vec::new();
    serve(&mut store, &mut input, &mut out).unwrap();
    assert_eq!(out, b"+PONG\r\n+OK\r\n:6\r\n$1\r\n6\r\n");
    assert_eq!(input, b"*2\r\n$3\r\nGE"); // partial command stays buffered
    input.extend_from_slice(b"T\r\n$1\r\nk\r\n");
    serve(&mut store, &mut input, &mut out).unwrap();
    assert!(out.ends_with(b"$1\r\n6\r\n") && input.is_empty());
    println!("wire: {:?}", String::from_utf8_lossy(&out));
    // protocol error: reply then caller closes
    let (mut bad, mut out2) = (b"$abc\r\n".to_vec(), Vec::new());
    assert!(serve(&mut store, &mut bad, &mut out2).is_err());
    assert!(out2.starts_with(b"-ERR Protocol error"));
}

fn main() {
    test_round_trip_and_splits();
    println!("round trip, every-prefix, byte-by-byte and pipelining: ok");
    test_limits_and_malformed();
    println!("limits (bulk, array, depth 100000, line) enforced: ok");
    test_hostile();
    test_glob();
    run_script();
    test_fuzz();
    println!("all RESP checks passed");
}
