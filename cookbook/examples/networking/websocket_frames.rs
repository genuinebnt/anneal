//! RECIPE 190 (protocols): WebSocket framing (RFC 6455): handshake accept key (SHA-1 + base64), frames, masking, fragmentation.
//! LEVEL: intermediate
//! Use in: chat/live-update servers, game and trading feeds, proxies, any full-duplex message protocol over TCP.   Run: cargo run --release --example websocket_frames
#![allow(dead_code)]
//!
//! WHAT: after an HTTP/1.1 `Upgrade: websocket` handshake the TCP stream carries FRAMES. A frame is
//! `FIN|RSV(3)|opcode(4)`, `MASK|len7`, an optional 16-bit (len7 = 126) or 64-bit (127) big-endian length, an optional 4-byte
//! masking key, then the payload. Opcodes: 0 continuation, 1 text, 2 binary, 8 close, 9 ping, 10 pong.
//! HANDSHAKE: the server proves it understood the upgrade by returning `base64(SHA-1(client_key + GUID))` in
//! `Sec-WebSocket-Accept` (RFC 6455 section 1.3). It is not security; it stops a non-WebSocket server (or a confused cache)
//! from being tricked into "agreeing". SHA-1 and base64 are written from scratch below and checked against the RFC example.
//! MASKING: every client-to-server frame MUST be masked with a fresh random 4-byte key: `payload[i] ^= key[i % 4]`. XOR is
//! its own inverse, so mask and unmask are the same function. The purpose is to stop a malicious page from choosing bytes that
//! look like HTTP to a buggy transparent proxy (cache poisoning). Server frames are NOT masked; a server must close on an
//! unmasked client frame and vice versa.
//! FRAGMENTATION: a message may be split across frames (first frame has the data opcode, later ones are continuations,
//! the last has FIN). Control frames (ping/pong/close) may be interleaved BETWEEN fragments, must have FIN and at most 125
//! payload bytes. Text messages must be valid UTF-8 (we check incrementally and fail fast on a definitely-bad prefix).
//! HOSTILE INPUT: the declared length is attacker-controlled, so it is compared to `max_payload` BEFORE anything is
//! allocated; non-minimal lengths, RSV bits, unknown opcodes and bad close codes are protocol errors that map to a close code.
//! COMMON USES: browser-to-server live channels (chat, notifications, collaborative editing, dashboards), market-data and
//! game-state streams, GraphQL subscriptions, reverse proxies that forward Upgrade connections, and test clients. Rust
//! production crates: tungstenite / tokio-tungstenite, fastwebsockets.
//! Read: https://datatracker.ietf.org/doc/html/rfc6455

// ---------------------------------------------------------------- SHA-1 and base64 (from scratch)

fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for block in msg.chunks(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes(block[4 * i..4 * i + 4].try_into().unwrap());
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (i, wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | (!b & d), 0x5A827999),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6u32),
            };
            let t = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(*wi);
            (e, d, c, b, a) = (d, c, b.rotate_left(30), a, t);
        }
        for (x, y) in h.iter_mut().zip([a, b, c, d, e]) {
            *x = x.wrapping_add(y);
        }
    }
    let mut out = [0u8; 20];
    for (i, x) in h.iter().enumerate() {
        out[4 * i..4 * i + 4].copy_from_slice(&x.to_be_bytes());
    }
    out
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64(data: &[u8]) -> String {
    let mut s = String::new();
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16
            | (*c.get(1).unwrap_or(&0) as u32) << 8
            | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= c.len() {
                s.push(B64[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

const GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

fn accept_key(client_key: &str) -> String {
    base64(&sha1(format!("{client_key}{GUID}").as_bytes()))
}

/// Minimal server-side handshake: validate the upgrade request and build the 101 response.
fn server_handshake(req: &str) -> Result<String, WsError> {
    let mut lines = req.split("\r\n");
    if !lines
        .next()
        .is_some_and(|l| l.starts_with("GET ") && l.ends_with(" HTTP/1.1"))
    {
        return Err(WsError::BadHandshake);
    }
    let (mut upgrade, mut conn, mut version, mut key) = (false, false, false, None);
    for l in lines {
        let Some((k, v)) = l.split_once(':') else {
            continue;
        };
        let (k, v) = (k.trim().to_ascii_lowercase(), v.trim());
        match k.as_str() {
            "upgrade" => upgrade = v.eq_ignore_ascii_case("websocket"),
            "connection" => {
                conn = v
                    .split(',')
                    .any(|t| t.trim().eq_ignore_ascii_case("upgrade"))
            }
            "sec-websocket-version" => version = v == "13",
            "sec-websocket-key" => key = Some(v),
            _ => {}
        }
    }
    // The key is 16 random bytes in base64: exactly 24 chars ending "==".
    let key = key.filter(|k| {
        k.len() == 24 && k.ends_with("==") && k.bytes().all(|b| B64.contains(&b) || b == b'=')
    });
    match (upgrade && conn && version, key) {
        (true, Some(k)) => Ok(format!(
            "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n",
            accept_key(k)
        )),
        _ => Err(WsError::BadHandshake),
    }
}

// ---------------------------------------------------------------- frames

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WsError {
    Rsv,
    BadOpcode,
    MaskMismatch,
    NonMinimalLength,
    BadLength,
    BadControl,
    TooBig,
    UnexpectedContinuation,
    ExpectedContinuation,
    BadUtf8,
    BadClose,
    BadHandshake,
    InvalidState,
}

impl WsError {
    /// The close code we send when this error ends the connection (RFC 6455 section 7.4.1).
    fn close_code(self) -> u16 {
        match self {
            WsError::BadUtf8 => 1007,
            WsError::TooBig => 1009,
            _ => 1002,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Opcode {
    Continuation = 0,
    Text = 1,
    Binary = 2,
    Close = 8,
    Ping = 9,
    Pong = 10,
}

impl Opcode {
    fn from_u8(v: u8) -> Result<Opcode, WsError> {
        Ok(match v {
            0 => Opcode::Continuation,
            1 => Opcode::Text,
            2 => Opcode::Binary,
            8 => Opcode::Close,
            9 => Opcode::Ping,
            10 => Opcode::Pong,
            _ => return Err(WsError::BadOpcode),
        })
    }
    fn is_control(self) -> bool {
        self as u8 >= 8
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Frame {
    fin: bool,
    opcode: Opcode,
    payload: Vec<u8>,
}

/// Mask and unmask are the same operation: XOR with the repeating 4-byte key.
fn apply_mask(data: &mut [u8], key: [u8; 4]) {
    for (i, b) in data.iter_mut().enumerate() {
        *b ^= key[i % 4];
    }
}

fn encode_frame(f: &Frame, mask: Option<[u8; 4]>) -> Vec<u8> {
    let mut out = vec![(f.fin as u8) << 7 | f.opcode as u8];
    let m = (mask.is_some() as u8) << 7;
    match f.payload.len() {
        n if n < 126 => out.push(m | n as u8),
        n if n <= 0xffff => {
            out.push(m | 126);
            out.extend_from_slice(&(n as u16).to_be_bytes());
        }
        n => {
            out.push(m | 127);
            out.extend_from_slice(&(n as u64).to_be_bytes());
        }
    }
    match mask {
        Some(k) => {
            out.extend_from_slice(&k);
            let mut p = f.payload.clone();
            apply_mask(&mut p, k);
            out.extend_from_slice(&p);
        }
        None => out.extend_from_slice(&f.payload),
    }
    out
}

/// `need_mask` is true when we are the server (client frames must be masked) and false when we are the client.
fn decode_frame(
    buf: &[u8],
    max_payload: usize,
    need_mask: bool,
) -> Result<Option<(Frame, usize)>, WsError> {
    if buf.len() < 2 {
        return Ok(None);
    }
    let (b0, b1) = (buf[0], buf[1]);
    if b0 & 0x70 != 0 {
        return Err(WsError::Rsv); // no extensions negotiated
    }
    let opcode = Opcode::from_u8(b0 & 0x0f)?;
    let fin = b0 & 0x80 != 0;
    if (b1 & 0x80 != 0) != need_mask {
        return Err(WsError::MaskMismatch);
    }
    let mut pos = 2;
    let len: u64 = match b1 & 0x7f {
        126 => {
            let Some(b) = buf.get(2..4) else {
                return Ok(None);
            };
            pos = 4;
            let v = u16::from_be_bytes(b.try_into().unwrap());
            if v < 126 {
                return Err(WsError::NonMinimalLength);
            }
            v as u64
        }
        127 => {
            let Some(b) = buf.get(2..10) else {
                return Ok(None);
            };
            pos = 10;
            let v = u64::from_be_bytes(b.try_into().unwrap());
            if v >> 63 == 1 {
                return Err(WsError::BadLength);
            }
            if v <= 0xffff {
                return Err(WsError::NonMinimalLength);
            }
            v
        }
        n => n as u64,
    };
    if opcode.is_control() && (!fin || len > 125) {
        return Err(WsError::BadControl);
    }
    if len > max_payload as u64 {
        return Err(WsError::TooBig); // before allocating or waiting for the bytes
    }
    let key = if need_mask {
        let Some(k) = buf.get(pos..pos + 4) else {
            return Ok(None);
        };
        pos += 4;
        Some(<[u8; 4]>::try_from(k).unwrap())
    } else {
        None
    };
    let end = pos + len as usize;
    if buf.len() < end {
        return Ok(None);
    }
    let mut payload = buf[pos..end].to_vec();
    if let Some(k) = key {
        apply_mask(&mut payload, k);
    }
    Ok(Some((
        Frame {
            fin,
            opcode,
            payload,
        },
        end,
    )))
}

// ---------------------------------------------------------------- messages

#[derive(Debug, Clone, PartialEq)]
enum Event {
    Text(String),
    Binary(Vec<u8>),
    Ping(Vec<u8>),
    Pong(Vec<u8>),
    Close(Option<(u16, String)>),
}

fn valid_close_code(c: u16) -> bool {
    matches!(c, 1000..=1003 | 1007..=1014 | 3000..=4999) // 1004/1005/1006/1015 are reserved and never sent on the wire
}

fn parse_close(p: &[u8]) -> Result<Event, WsError> {
    match p.len() {
        0 => Ok(Event::Close(None)),
        1 => Err(WsError::BadClose),
        _ => {
            let code = u16::from_be_bytes([p[0], p[1]]);
            let reason = std::str::from_utf8(&p[2..]).map_err(|_| WsError::BadUtf8)?;
            if !valid_close_code(code) {
                return Err(WsError::BadClose);
            }
            Ok(Event::Close(Some((code, reason.to_string()))))
        }
    }
}

/// Reassembles fragmented messages; control frames pass straight through between fragments.
struct Assembler {
    partial: Option<(Opcode, Vec<u8>, usize)>, // (kind, bytes so far, bytes already UTF-8 checked)
    max_message: usize,
}

impl Assembler {
    fn push(&mut self, f: Frame) -> Result<Option<Event>, WsError> {
        match f.opcode {
            Opcode::Ping => return Ok(Some(Event::Ping(f.payload))),
            Opcode::Pong => return Ok(Some(Event::Pong(f.payload))),
            Opcode::Close => return parse_close(&f.payload).map(Some),
            Opcode::Text | Opcode::Binary => {
                if self.partial.is_some() {
                    return Err(WsError::ExpectedContinuation);
                }
                self.partial = Some((f.opcode, Vec::new(), 0));
            }
            Opcode::Continuation => {
                if self.partial.is_none() {
                    return Err(WsError::UnexpectedContinuation);
                }
            }
        }
        let (kind, buf, checked) = self.partial.as_mut().unwrap();
        if buf.len() + f.payload.len() > self.max_message {
            return Err(WsError::TooBig);
        }
        buf.extend_from_slice(&f.payload);
        if *kind == Opcode::Text {
            // Incremental UTF-8: an incomplete trailing sequence is fine for now, a definitely-invalid one fails fast.
            match std::str::from_utf8(&buf[*checked..]) {
                Ok(_) => *checked = buf.len(),
                Err(e) if e.error_len().is_none() => *checked += e.valid_up_to(),
                Err(_) => return Err(WsError::BadUtf8),
            }
        }
        if !f.fin {
            return Ok(None);
        }
        let (kind, buf, checked) = self.partial.take().unwrap();
        if kind == Opcode::Text {
            if checked != buf.len() {
                return Err(WsError::BadUtf8); // message ended in the middle of a code point
            }
            return Ok(Some(Event::Text(
                String::from_utf8(buf).map_err(|_| WsError::BadUtf8)?,
            )));
        }
        Ok(Some(Event::Binary(buf)))
    }
}

// ---------------------------------------------------------------- connection state machine

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    Server,
    Client,
}

/// Connecting -> Open -> Closing (we sent Close, waiting for the echo) -> Closed. Receiving Close while Open
/// makes us echo it and go straight to Closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Connecting,
    Open,
    Closing,
    Closed,
}

struct Conn {
    role: Role,
    state: State,
    buf: Vec<u8>,
    asm: Assembler,
    max_payload: usize,
    outbox: Vec<u8>,
    error: Option<WsError>,
    mask_ctr: u32,
}

impl Conn {
    fn new(role: Role, max_payload: usize) -> Conn {
        Conn {
            role,
            state: State::Connecting,
            buf: Vec::new(),
            asm: Assembler {
                partial: None,
                max_message: max_payload * 4,
            },
            max_payload,
            outbox: Vec::new(),
            error: None,
            mask_ctr: 1,
        }
    }
    fn open(&mut self) {
        assert_eq!(self.state, State::Connecting);
        self.state = State::Open;
    }
    fn queue(&mut self, f: Frame) {
        let mask = (self.role == Role::Client).then(|| {
            self.mask_ctr = self.mask_ctr.wrapping_add(1);
            self.mask_ctr.wrapping_mul(0x9E37_79B1).to_be_bytes() // real clients use a CSPRNG
        });
        self.outbox.extend(encode_frame(&f, mask));
    }
    fn close_frame(code: u16, reason: &str) -> Frame {
        let mut p = code.to_be_bytes().to_vec();
        p.extend_from_slice(&reason.as_bytes()[..reason.len().min(123)]);
        Frame {
            fin: true,
            opcode: Opcode::Close,
            payload: p,
        }
    }
    fn send(&mut self, text: Option<&str>, bin: &[u8]) -> Result<(), WsError> {
        if self.state != State::Open {
            return Err(WsError::InvalidState);
        }
        let (opcode, payload) = match text {
            Some(t) => (Opcode::Text, t.as_bytes().to_vec()),
            None => (Opcode::Binary, bin.to_vec()),
        };
        self.queue(Frame {
            fin: true,
            opcode,
            payload,
        });
        Ok(())
    }
    fn send_close(&mut self, code: u16, reason: &str) -> Result<(), WsError> {
        if self.state != State::Open {
            return Err(WsError::InvalidState);
        }
        self.queue(Self::close_frame(code, reason));
        self.state = State::Closing;
        Ok(())
    }
    fn take_outbox(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.outbox)
    }

    /// Feed received bytes (any chunking); returns the events completed by them.
    fn feed(&mut self, bytes: &[u8]) -> Vec<Event> {
        let mut events = Vec::new();
        if self.state == State::Closed || self.error.is_some() {
            return events;
        }
        self.buf.extend_from_slice(bytes);
        while self.state != State::Closed {
            let step = decode_frame(&self.buf, self.max_payload, self.role == Role::Server)
                .and_then(|r| r.map(|(f, n)| self.asm.push(f).map(|e| (e, n))).transpose());
            match step {
                Ok(None) => break,
                Ok(Some((ev, n))) => {
                    self.buf.drain(..n);
                    match &ev {
                        Some(Event::Ping(p)) => self.queue(Frame {
                            fin: true,
                            opcode: Opcode::Pong,
                            payload: p.clone(),
                        }),
                        Some(Event::Close(c)) => {
                            if self.state == State::Open {
                                let code = c.as_ref().map_or(1000, |c| c.0);
                                self.queue(Self::close_frame(code, ""));
                            }
                            self.state = State::Closed;
                        }
                        _ => {}
                    }
                    events.extend(ev);
                }
                Err(e) => {
                    self.error = Some(e);
                    if self.state == State::Open {
                        self.queue(Self::close_frame(e.close_code(), ""));
                    }
                    self.state = State::Closed;
                }
            }
        }
        events
    }
}

// ---------------------------------------------------------------- demo / tests

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

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn fr(fin: bool, opcode: Opcode, p: &[u8]) -> Frame {
    Frame {
        fin,
        opcode,
        payload: p.to_vec(),
    }
}

fn main() {
    // 1. SHA-1, base64 and the RFC 6455 handshake example.
    assert_eq!(
        hex(&sha1(b"abc")),
        "a9993e364706816aba3e25717850c26c9cd0d89d"
    );
    assert_eq!(
        hex(&sha1(&[b'a'; 1000])),
        "291e9a6c66994949b57ba5e650361e98fc36b1ba"
    );
    assert_eq!(
        (base64(b"M"), base64(b"Ma"), base64(b"Man")),
        ("TQ==".into(), "TWE=".into(), "TWFu".into())
    );
    assert_eq!(
        accept_key("dGhlIHNhbXBsZSBub25jZQ=="),
        "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
    );
    let req = "GET /chat HTTP/1.1\r\nHost: server.example.com\r\nUpgrade: websocket\r\nConnection: keep-alive, Upgrade\r\n\
               Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n";
    let resp = server_handshake(req).unwrap();
    assert!(resp.contains("Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=\r\n"));
    assert!(server_handshake(&req.replace("13", "8")).is_err());
    println!(
        "handshake accept = {}",
        accept_key("dGhlIHNhbXBsZSBub25jZQ==")
    );

    // 2. RFC 6455 section 5.7 example frames, byte for byte.
    let hello = fr(true, Opcode::Text, b"Hello");
    assert_eq!(
        encode_frame(&hello, None),
        [0x81, 0x05, 0x48, 0x65, 0x6c, 0x6c, 0x6f]
    );
    let key = [0x37, 0xfa, 0x21, 0x3d];
    let masked = encode_frame(&hello, Some(key));
    assert_eq!(
        masked,
        [
            0x81, 0x85, 0x37, 0xfa, 0x21, 0x3d, 0x7f, 0x9f, 0x4d, 0x51, 0x58
        ]
    );
    assert_eq!(
        decode_frame(&masked, 1024, true).unwrap().unwrap(),
        (hello.clone(), masked.len())
    );
    assert_eq!(
        encode_frame(&fr(true, Opcode::Ping, b"Hello"), None),
        [0x89, 0x05, 0x48, 0x65, 0x6c, 0x6c, 0x6f]
    );
    assert_eq!(
        &encode_frame(&fr(true, Opcode::Binary, &[0; 256]), None)[..4],
        [0x82, 0x7e, 0x01, 0x00]
    );
    assert_eq!(
        &encode_frame(&fr(true, Opcode::Binary, &vec![0; 65536]), None)[..10],
        [0x82, 0x7f, 0, 0, 0, 0, 0, 1, 0, 0]
    );
    let mut d = *b"mask me";
    apply_mask(&mut d, key);
    apply_mask(&mut d, key);
    assert_eq!(&d, b"mask me"); // XOR twice = identity

    // 3. Length forms round trip (7-bit, 16-bit, 64-bit) and violations.
    for n in [0, 1, 125, 126, 127, 65535, 65536, 70000] {
        let f = fr(true, Opcode::Binary, &vec![0xAB; n]);
        for m in [None, Some(key)] {
            let bytes = encode_frame(&f, m);
            assert_eq!(
                decode_frame(&bytes, 1 << 20, m.is_some()).unwrap().unwrap(),
                (f.clone(), bytes.len())
            );
        }
    }
    let huge = [0x82, 0x7f, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0]; // declares 2^40 bytes
    assert_eq!(
        decode_frame(&huge, 1 << 20, false).unwrap_err(),
        WsError::TooBig
    );
    assert_eq!(
        decode_frame(&[0x82, 0x7e, 0, 5], 1 << 20, false).unwrap_err(),
        WsError::NonMinimalLength
    );
    assert_eq!(
        decode_frame(&[0xc1, 0x00], 10, false).unwrap_err(),
        WsError::Rsv
    );
    assert_eq!(
        decode_frame(&[0x83, 0x00], 10, false).unwrap_err(),
        WsError::BadOpcode
    );
    assert_eq!(
        decode_frame(&[0x09, 0x00], 10, false).unwrap_err(),
        WsError::BadControl
    ); // fragmented ping
    assert_eq!(
        decode_frame(&[0x89, 0x7e, 0, 200], 1000, false).unwrap_err(),
        WsError::BadControl
    ); // big ping
    assert_eq!(
        decode_frame(&[0x81, 0x00], 10, true).unwrap_err(),
        WsError::MaskMismatch
    ); // unmasked client frame

    // 4. Fragmentation with an interleaved ping, UTF-8 handling, close-code validation.
    let mut asm = Assembler {
        partial: None,
        max_message: 1 << 20,
    };
    let seq = [
        fr(false, Opcode::Text, b"Hel"),
        fr(true, Opcode::Ping, b"p"),
        fr(false, Opcode::Continuation, b"lo "),
        fr(true, Opcode::Continuation, "w\u{f6}rld".as_bytes()),
    ];
    let evs: Vec<Event> = seq
        .into_iter()
        .filter_map(|f| asm.push(f).unwrap())
        .collect();
    assert_eq!(
        evs,
        vec![
            Event::Ping(b"p".to_vec()),
            Event::Text("Hello w\u{f6}rld".into())
        ]
    );
    let euro = "\u{20ac}".as_bytes(); // 3 bytes split 1+2 across fragments: legal
    let mut asm = Assembler {
        partial: None,
        max_message: 100,
    };
    assert_eq!(asm.push(fr(false, Opcode::Text, &euro[..1])), Ok(None));
    assert_eq!(
        asm.push(fr(true, Opcode::Continuation, &euro[1..])),
        Ok(Some(Event::Text("\u{20ac}".into())))
    );
    let mut asm = Assembler {
        partial: None,
        max_message: 100,
    };
    assert_eq!(
        asm.push(fr(false, Opcode::Text, &[0xff])),
        Err(WsError::BadUtf8)
    ); // fail fast
    let mut asm = Assembler {
        partial: None,
        max_message: 100,
    };
    assert_eq!(
        asm.push(fr(true, Opcode::Text, &euro[..2])),
        Err(WsError::BadUtf8)
    ); // ends mid code point
    let mut asm = Assembler {
        partial: None,
        max_message: 100,
    };
    assert_eq!(
        asm.push(fr(true, Opcode::Continuation, b"x")),
        Err(WsError::UnexpectedContinuation)
    );
    let mut asm = Assembler {
        partial: None,
        max_message: 100,
    };
    asm.push(fr(false, Opcode::Binary, b"a")).unwrap();
    assert_eq!(
        asm.push(fr(true, Opcode::Text, b"b")),
        Err(WsError::ExpectedContinuation)
    );
    assert_eq!(
        parse_close(&[0x03, 0xe8, b'b', b'y', b'e']),
        Ok(Event::Close(Some((1000, "bye".into()))))
    );
    assert_eq!(parse_close(&[0x03, 0xed]), Err(WsError::BadClose)); // 1005 never on the wire
    assert_eq!(parse_close(&[0x03]), Err(WsError::BadClose));
    println!("fragments, interleaved control frames, UTF-8 and close codes behave");

    // 5. Whole lifecycle: client <-> server in memory, delivered one byte at a time.
    let (mut client, mut server) = (
        Conn::new(Role::Client, 1 << 16),
        Conn::new(Role::Server, 1 << 16),
    );
    client.open();
    server.open();
    client.send(Some("ping me"), &[]).unwrap();
    client.send(None, &[1, 2, 3]).unwrap();
    client.queue(fr(true, Opcode::Ping, b"hb"));
    let wire = client.take_outbox();
    assert!(wire[1] & 0x80 != 0, "client frames are masked");
    let got: Vec<Event> = wire.iter().flat_map(|b| server.feed(&[*b])).collect();
    assert_eq!(
        got,
        vec![
            Event::Text("ping me".into()),
            Event::Binary(vec![1, 2, 3]),
            Event::Ping(b"hb".to_vec())
        ]
    );
    let back = server.take_outbox(); // the automatic pong (unmasked)
    assert_eq!(client.feed(&back), vec![Event::Pong(b"hb".to_vec())]);
    server.send(Some("reply"), &[]).unwrap();
    assert_eq!(
        client.feed(&server.take_outbox()),
        vec![Event::Text("reply".into())]
    );
    client.send_close(1000, "done").unwrap();
    assert_eq!(client.state, State::Closing);
    assert_eq!(client.send(Some("late"), &[]), Err(WsError::InvalidState));
    assert_eq!(
        server.feed(&client.take_outbox()),
        vec![Event::Close(Some((1000, "done".into())))]
    );
    assert_eq!(server.state, State::Closed);
    assert_eq!(
        client.feed(&server.take_outbox()),
        vec![Event::Close(Some((1000, String::new())))]
    );
    assert_eq!(
        (client.state, client.error, server.error),
        (State::Closed, None, None)
    );
    println!("lifecycle: Open -> Closing -> Closed on both ends, no error");

    // 6. A protocol violation closes with the right code (unmasked client frame: 1002; bad UTF-8: 1007).
    let mut s = Conn::new(Role::Server, 1024);
    s.open();
    s.feed(&encode_frame(&hello, None));
    assert_eq!(
        (s.state, s.error),
        (State::Closed, Some(WsError::MaskMismatch))
    );
    assert_eq!(s.take_outbox(), [0x88, 0x02, 0x03, 0xea]); // Close 1002
    let mut s = Conn::new(Role::Server, 1024);
    s.open();
    s.feed(&encode_frame(
        &fr(true, Opcode::Text, &[0xc3, 0x28]),
        Some(key),
    ));
    assert_eq!(s.take_outbox(), [0x88, 0x02, 0x03, 0xef]); // Close 1007

    // 7. Fuzz: random and mutated client streams; chunking never changes the outcome; nothing panics or loops.
    let mut rng = Rng(0xFEED_FACE_CAFE);
    let mut stream = Vec::new();
    let mk = |f: Frame, k: u8| encode_frame(&f, Some([k, k ^ 0x55, 3, 9]));
    for f in [
        fr(false, Opcode::Text, b"frag"),
        fr(true, Opcode::Ping, b"pp"),
        fr(true, Opcode::Continuation, b"ment"),
        fr(true, Opcode::Binary, &[7; 300]),
        fr(true, Opcode::Close, &[3, 232]),
    ] {
        stream.extend(mk(f, 0x11));
    }
    let (mut errs, mut evs_total) = (0, 0);
    for i in 0..20_000 {
        let input: Vec<u8> = if i % 4 == 0 {
            (0..rng.below(80)).map(|_| rng.next() as u8).collect()
        } else {
            let mut v = stream.clone();
            for _ in 0..1 + rng.below(4) {
                let at = rng.below(v.len());
                match rng.below(3) {
                    0 => v[at] = rng.next() as u8,
                    1 => v.truncate(at),
                    _ => v.insert(at, rng.next() as u8),
                }
                if v.is_empty() {
                    v.push(0x81);
                }
            }
            v
        };
        let mut a = Conn::new(Role::Server, 512);
        a.open();
        let ea = a.feed(&input);
        let mut b = Conn::new(Role::Server, 512);
        b.open();
        let eb: Vec<Event> = input.iter().flat_map(|x| b.feed(&[*x])).collect();
        assert_eq!(
            (&ea, a.error, a.state, a.take_outbox()),
            (&eb, b.error, b.state, b.take_outbox()),
            "input {i}"
        );
        errs += a.error.is_some() as u32;
        evs_total += ea.len();
    }
    println!(
        "fuzz: 20000 inputs, {evs_total} events, {errs} protocol errors, byte-wise == all-at-once"
    );
}
