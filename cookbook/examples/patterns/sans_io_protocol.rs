//! RECIPE 215 (patterns): sans-IO protocol design: a pure state machine driven by a simulator, a blocking socket and tokio.
//! LEVEL: advanced
//! Use in: protocol implementations (HTTP/2, QUIC, TLS, WebRTC), anything with timers and retransmission that must be unit-testable.   Run: cargo run --release --example sans_io_protocol
#![allow(dead_code)]
//!
//! SANS-IO means the protocol logic never touches a socket, a thread, an async runtime or the system clock. It is a plain struct:
//!   handle_input(now, bytes)   feed bytes that arrived from the peer
//!   poll_transmit()            take bytes that must be sent (queue drained by the caller)
//!   poll_event()               take application-level events (connected, response, closed)
//!   handle_timeout(now)        tell the machine that time has advanced (it fires whatever is due)
//!   poll_timeout()             ask when it next needs `handle_timeout` (the caller sleeps until then)
//! Time is an INPUT (`Instant` values the caller passes in), so tests use a virtual clock and a seeded lossy network and finish in
//! without waiting, deterministically, with no sockets and no `sleep`. The SAME code then runs under a blocking `TcpStream` loop
//! and a tokio loop: only the short driver differs. `quinn-proto` and `str0m` are built this way, and `rustls` exposes buffer
//! methods that leave the IO to the caller.
//!
//! Why this beats embedding IO in the protocol: (1) tests need no network and no flaky timing; (2) fuzzing/property tests can inject
//! loss, duplication, reordering and clock jumps; (3) one implementation serves every runtime; (4) the machine owns no tasks or locks,
//! so there are no hidden suspension points, cancellation hazards or runtime coupling; (5) the caller controls buffering and syscalls
//! (batching, `sendmmsg`, zero-copy). The cost: the caller must write the (small) driver loop, and APIs are a bit more verbose.
//!
//! Protocol here (line based, `\n` terminated): client `HELLO <id>` -> server `WELCOME`; then `REQ <seq> <text>` -> `RES <seq> <TEXT>`;
//! `PING` -> `PONG` keeps idle links alive; no input for `idle` => close. The client RETRANSMITS HELLO/REQ every `rto` until answered
//! (max `max_retries`), the server is idempotent so duplicates and reordering are harmless.
//! A line is at most 1024 bytes. After any error from `handle_input`, drop the connection.
//! LIMITS: the server remembers every sequence number, and the output queues grow until the caller drains them. Any bytes restart
//! the idle timer. The drivers do not time out writes, and they return `Ok` when the peer closes, even with requests unanswered.
//! COMMON USES: QUIC and TLS libraries, WebRTC/SIP/DNS clients, embedded or `no_std` protocol cores, deterministic simulation
//! testing of distributed systems (the idea behind FoundationDB-style simulation).
//! Read: https://www.firezone.dev/blog/sans-io , https://sans-io.readthedocs.io/ , https://docs.rs/quinn-proto , https://docs.rs/str0m
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::fmt;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

// ------------------------------------------------------------- common types
#[derive(Debug, Clone, PartialEq)]
enum Error {
    LineTooLong,
    Protocol(String),
    BadPayload,
    Closed,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Reason {
    Timeout,
    RetriesExhausted,
}
#[derive(Debug, Clone, PartialEq)]
enum Event {
    Connected,
    Response { seq: u32, body: String },
    Request { seq: u32, body: String },
    Closed(Reason),
}
#[derive(Clone, Copy)]
struct Config {
    rto: Duration,
    keepalive: Duration,
    idle: Duration,
    max_retries: u32,
}
impl Default for Config {
    fn default() -> Self {
        Config {
            rto: Duration::from_millis(200),
            keepalive: Duration::from_secs(1),
            idle: Duration::from_secs(5),
            max_retries: 20,
        }
    }
}
#[derive(Default, Debug)]
struct Stats {
    retransmits: u32,
    pings: u32,
}

/// The sans-IO surface shared by client and server so ONE driver function serves both.
trait Sans {
    fn handle_input(&mut self, now: Instant, bytes: &[u8]) -> Result<(), Error>;
    fn poll_transmit(&mut self) -> Option<Vec<u8>>;
    fn poll_event(&mut self) -> Option<Event>;
    fn handle_timeout(&mut self, now: Instant);
    fn poll_timeout(&self) -> Option<Instant>;
    fn is_closed(&self) -> bool;
}

/// Line framing: works for datagrams (whole lines) and byte streams (lines split across reads).
#[derive(Default)]
struct Framer {
    buf: Vec<u8>,
}
impl Framer {
    const MAX: usize = 1024;
    fn push(&mut self, b: &[u8]) {
        self.buf.extend_from_slice(b);
    }
    fn next_line(&mut self) -> Result<Option<String>, Error> {
        match self.buf.iter().position(|&c| c == b'\n') {
            // the limit must not depend on whether the newline came in the same read
            Some(i) if i > Self::MAX => Err(Error::LineTooLong),
            Some(i) => {
                let line: Vec<u8> = self.buf.drain(..=i).collect();
                let s =
                    std::str::from_utf8(&line[..i]).map_err(|_| Error::Protocol("utf8".into()))?;
                Ok(Some(s.to_string()))
            }
            None if self.buf.len() > Self::MAX => Err(Error::LineTooLong),
            None => Ok(None),
        }
    }
}
fn min_opt(a: Option<Instant>, b: Instant) -> Option<Instant> {
    Some(a.map_or(b, |a| a.min(b)))
}

// ------------------------------------------------------------------- client
enum CState {
    Hello { deadline: Instant, tries: u32 },
    Ready,
    Closed,
}
struct Inflight {
    body: String,
    deadline: Instant,
    tries: u32,
}
struct Client {
    cfg: Config,
    state: CState,
    framer: Framer,
    out: VecDeque<Vec<u8>>,
    events: VecDeque<Event>,
    pending: Vec<(u32, String)>, // queued before the handshake finished
    inflight: BTreeMap<u32, Inflight>,
    next_seq: u32,
    last_rx: Instant,
    last_tx: Instant,
    stats: Stats,
}
impl Client {
    fn new(cfg: Config, now: Instant) -> Self {
        let mut c = Client {
            cfg,
            state: CState::Hello {
                deadline: now + cfg.rto,
                tries: 0,
            },
            framer: Framer::default(),
            out: VecDeque::new(),
            events: VecDeque::new(),
            pending: vec![],
            inflight: BTreeMap::new(),
            next_seq: 1,
            last_rx: now,
            last_tx: now,
            stats: Stats::default(),
        };
        c.out.push_back(b"HELLO c1\n".to_vec());
        c
    }
    /// Queue a request; returns its sequence number.
    fn request(&mut self, now: Instant, body: &str) -> Result<u32, Error> {
        if matches!(self.state, CState::Closed) {
            return Err(Error::Closed);
        }
        let seq = self.next_seq;
        // the whole line must fit the peer's limit, and a newline would end it early
        if body.contains('\n') || format!("REQ {seq} ").len() + body.len() > Framer::MAX {
            return Err(Error::BadPayload);
        }
        self.next_seq += 1;
        match self.state {
            CState::Ready => self.send_req(now, seq, body.to_string()),
            _ => self.pending.push((seq, body.to_string())),
        }
        Ok(seq)
    }
    fn send_req(&mut self, now: Instant, seq: u32, body: String) {
        self.out
            .push_back(format!("REQ {seq} {body}\n").into_bytes());
        self.inflight.insert(
            seq,
            Inflight {
                body,
                deadline: now + self.cfg.rto,
                tries: 0,
            },
        );
        self.last_tx = now;
    }
    fn close(&mut self, r: Reason) {
        self.state = CState::Closed;
        self.events.push_back(Event::Closed(r));
    }
}
impl Sans for Client {
    fn handle_input(&mut self, now: Instant, bytes: &[u8]) -> Result<(), Error> {
        if self.is_closed() {
            return Ok(()); // nothing may follow the Closed event
        }
        self.last_rx = now;
        self.framer.push(bytes);
        while let Some(line) = self.framer.next_line()? {
            let (cmd, rest) = line.split_once(' ').unwrap_or((&line, ""));
            match cmd {
                "WELCOME" => {
                    if matches!(self.state, CState::Hello { .. }) {
                        self.state = CState::Ready;
                        self.events.push_back(Event::Connected);
                        for (seq, body) in std::mem::take(&mut self.pending) {
                            self.send_req(now, seq, body);
                        }
                    } // a duplicate WELCOME (retransmitted HELLO) is ignored
                }
                "RES" => {
                    let (seq, body) = rest.split_once(' ').unwrap_or((rest, ""));
                    let seq: u32 = seq.parse().map_err(|_| Error::Protocol(line.clone()))?;
                    if self.inflight.remove(&seq).is_some() {
                        self.events.push_back(Event::Response {
                            seq,
                            body: body.to_string(),
                        });
                    } // else: duplicate response, already delivered
                }
                "PONG" => {}
                _ => return Err(Error::Protocol(line)),
            }
        }
        Ok(())
    }
    fn poll_transmit(&mut self) -> Option<Vec<u8>> {
        self.out.pop_front()
    }
    fn poll_event(&mut self) -> Option<Event> {
        self.events.pop_front()
    }
    fn handle_timeout(&mut self, now: Instant) {
        if matches!(self.state, CState::Closed) {
            return;
        }
        if now >= self.last_rx + self.cfg.idle {
            return self.close(Reason::Timeout);
        }
        let rto = self.cfg.rto;
        let max = self.cfg.max_retries;
        match &mut self.state {
            CState::Hello { deadline, tries } if now >= *deadline => {
                *tries += 1;
                if *tries > max {
                    return self.close(Reason::RetriesExhausted);
                }
                *deadline = now + rto;
                self.stats.retransmits += 1;
                self.last_tx = now;
                self.out.push_back(b"HELLO c1\n".to_vec());
            }
            CState::Ready => {
                let mut exhausted = false;
                for (seq, f) in self.inflight.iter_mut().filter(|(_, f)| now >= f.deadline) {
                    f.tries += 1;
                    if f.tries > max {
                        exhausted = true;
                        break;
                    }
                    f.deadline = now + rto;
                    self.stats.retransmits += 1;
                    self.last_tx = now;
                    self.out
                        .push_back(format!("REQ {seq} {}\n", f.body).into_bytes());
                }
                if exhausted {
                    return self.close(Reason::RetriesExhausted);
                }
                if now >= self.last_tx + self.cfg.keepalive {
                    self.last_tx = now;
                    self.stats.pings += 1;
                    self.out.push_back(b"PING\n".to_vec());
                }
            }
            _ => {}
        }
    }
    fn poll_timeout(&self) -> Option<Instant> {
        let mut t = match self.state {
            CState::Closed => return None,
            _ => Some(self.last_rx + self.cfg.idle),
        };
        if let CState::Hello { deadline, .. } = self.state {
            t = min_opt(t, deadline);
        }
        if matches!(self.state, CState::Ready) {
            t = min_opt(t, self.last_tx + self.cfg.keepalive);
            for f in self.inflight.values() {
                t = min_opt(t, f.deadline);
            }
        }
        t
    }
    fn is_closed(&self) -> bool {
        matches!(self.state, CState::Closed)
    }
}

// ------------------------------------------------------------------- server
struct Server {
    cfg: Config,
    framer: Framer,
    out: VecDeque<Vec<u8>>,
    events: VecDeque<Event>,
    established: bool,
    seen: HashSet<u32>,
    last_rx: Instant,
    closed: bool,
}
impl Server {
    fn new(cfg: Config, now: Instant) -> Self {
        Server {
            cfg,
            framer: Framer::default(),
            out: VecDeque::new(),
            events: VecDeque::new(),
            established: false,
            seen: HashSet::new(),
            last_rx: now,
            closed: false,
        }
    }
}
impl Sans for Server {
    fn handle_input(&mut self, now: Instant, bytes: &[u8]) -> Result<(), Error> {
        if self.closed {
            return Ok(());
        }
        self.last_rx = now;
        self.framer.push(bytes);
        while let Some(line) = self.framer.next_line()? {
            let (cmd, rest) = line.split_once(' ').unwrap_or((&line, ""));
            match cmd {
                "HELLO" => {
                    if !self.established {
                        self.established = true;
                        self.events.push_back(Event::Connected);
                    }
                    self.out.push_back(b"WELCOME\n".to_vec()); // idempotent: every HELLO is answered
                }
                "REQ" if self.established => {
                    let (seq, body) = rest.split_once(' ').unwrap_or((rest, ""));
                    let seq: u32 = seq.parse().map_err(|_| Error::Protocol(line.clone()))?;
                    if self.seen.insert(seq) {
                        self.events.push_back(Event::Request {
                            seq,
                            body: body.to_string(),
                        });
                    }
                    // ASCII only: Unicode uppercasing can grow the text past the line limit
                    self.out.push_back(
                        format!("RES {seq} {}\n", body.to_ascii_uppercase()).into_bytes(),
                    );
                }
                "REQ" => {} // REQ overtook HELLO on the network: drop it, the client will retransmit
                "PING" => self.out.push_back(b"PONG\n".to_vec()),
                _ => return Err(Error::Protocol(line)),
            }
        }
        Ok(())
    }
    fn poll_transmit(&mut self) -> Option<Vec<u8>> {
        self.out.pop_front()
    }
    fn poll_event(&mut self) -> Option<Event> {
        self.events.pop_front()
    }
    fn handle_timeout(&mut self, now: Instant) {
        if !self.closed && now >= self.last_rx + self.cfg.idle {
            self.closed = true;
            self.events.push_back(Event::Closed(Reason::Timeout));
        }
    }
    fn poll_timeout(&self) -> Option<Instant> {
        (!self.closed).then_some(self.last_rx + self.cfg.idle)
    }
    fn is_closed(&self) -> bool {
        self.closed
    }
}

// ---------------------------------------------- (a) deterministic simulation
struct Rng(u64);
impl Rng {
    fn below(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % n
    }
}
struct Pkt {
    at: Instant,
    id: u64,
    to_server: bool,
    data: Vec<u8>,
}
struct Sim {
    start: Instant,
    now: Instant,
    c: Client,
    s: Server,
    rng: Rng,
    loss_pct: u64,
    flight: Vec<Pkt>,
    next_id: u64,
    cev: Vec<Event>,
    sev: Vec<Event>,
}
impl Sim {
    fn new(seed: u64, loss_pct: u64, cfg: Config) -> Self {
        let now = Instant::now(); // only a base: every later value is `base + virtual duration`
        Sim {
            start: now,
            now,
            c: Client::new(cfg, now),
            s: Server::new(cfg, now),
            rng: Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1),
            loss_pct,
            flight: vec![],
            next_id: 0,
            cev: vec![],
            sev: vec![],
        }
    }
    fn send(&mut self, to_server: bool, data: Vec<u8>) {
        if self.rng.below(100) < self.loss_pct {
            return; // dropped
        }
        let copies = if self.rng.below(100) < 5 { 2 } else { 1 }; // 5% duplicated
        for _ in 0..copies {
            let jitter = Duration::from_millis(10 + self.rng.below(70)); // wide jitter => reordering
            self.next_id += 1;
            self.flight.push(Pkt {
                at: self.now + jitter,
                id: self.next_id,
                to_server,
                data: data.clone(),
            });
        }
    }
    fn pump(&mut self) {
        while let Some(d) = self.c.poll_transmit() {
            self.send(true, d);
        }
        while let Some(d) = self.s.poll_transmit() {
            self.send(false, d);
        }
        while let Some(e) = self.c.poll_event() {
            self.cev.push(e);
        }
        while let Some(e) = self.s.poll_event() {
            self.sev.push(e);
        }
    }
    /// Advance to the next interesting instant (bounded by `limit`). Returns false when nothing is left before `limit`.
    fn step(&mut self, limit: Instant) -> bool {
        self.pump();
        let mut next = [
            self.c.poll_timeout(),
            self.s.poll_timeout(),
            self.flight.iter().map(|p| p.at).min(),
        ]
        .into_iter()
        .flatten()
        .min();
        next = next.map(|n| n.max(self.now));
        match next {
            Some(n) if n <= limit => self.now = n,
            _ => {
                self.now = limit;
                return false;
            }
        }
        let now = self.now;
        let (mut due, rest): (Vec<Pkt>, Vec<Pkt>) = std::mem::take(&mut self.flight)
            .into_iter()
            .partition(|p| p.at <= now);
        self.flight = rest;
        due.sort_by_key(|p| (p.at, p.id));
        for p in due {
            let r = if p.to_server {
                self.s.handle_input(now, &p.data)
            } else {
                self.c.handle_input(now, &p.data)
            };
            r.expect("simulated network never corrupts bytes");
        }
        self.c.handle_timeout(now);
        self.s.handle_timeout(now);
        self.pump();
        true
    }
    fn run_until(&mut self, limit: Duration) {
        let limit = self.start + limit;
        let mut guard = 0;
        while self.now < limit && self.step(limit) {
            guard += 1;
            assert!(guard < 200_000, "simulation did not converge");
        }
    }
    fn responses(&self) -> Vec<(u32, String)> {
        let mut v: Vec<_> = self
            .cev
            .iter()
            .filter_map(|e| match e {
                Event::Response { seq, body } => Some((*seq, body.clone())),
                _ => None,
            })
            .collect();
        v.sort();
        v
    }
}
/// Run a whole session under a lossy network; returns (virtual elapsed, retransmits, responses).
fn simulate(seed: u64, loss_pct: u64, n: usize) -> (Duration, u32, Vec<(u32, String)>) {
    let cfg = Config::default();
    let mut sim = Sim::new(seed, loss_pct, cfg);
    let t0 = sim.now;
    for i in 0..n {
        sim.c.request(sim.now, &format!("job {i}")).unwrap();
    }
    let mut guard = 0;
    while sim.responses().len() < n && !sim.c.is_closed() {
        assert!(sim.step(t0 + Duration::from_secs(120)), "stalled");
        guard += 1;
        assert!(guard < 200_000);
    }
    assert!(
        !sim.c.is_closed(),
        "seed {seed}: client gave up under {loss_pct}% loss"
    );
    (sim.now - t0, sim.c.stats.retransmits, sim.responses())
}

fn expected(n: usize) -> Vec<(u32, String)> {
    (0..n).map(|i| (i as u32 + 1, format!("JOB {i}"))).collect()
}
fn test_simulated() {
    // perfect network: no retransmissions at all, quick
    let (_, rt, resp) = simulate(1, 0, 10);
    assert_eq!((rt, resp), (0, expected(10)));
    // 20% loss + 5% duplication + reordering: every one of 20 seeds completes with exact results
    let mut total_rt = 0;
    for seed in 1..=20 {
        let (_, rt, resp) = simulate(seed, 20, 10);
        assert_eq!(resp, expected(10), "seed {seed}");
        total_rt += rt;
    }
    assert!(total_rt > 0, "loss must have forced retransmissions");
    // determinism: same seed => same virtual duration, retransmission count and responses
    assert_eq!(simulate(7, 20, 10), simulate(7, 20, 10));
    println!("sim: 20 seeds @20% loss ok, {total_rt} retransmits in total");

    // keep-alive: 20 virtual seconds of silence on a healthy link, nobody times out
    let mut sim = Sim::new(3, 0, Config::default());
    sim.run_until(Duration::from_secs(20));
    assert!(!sim.c.is_closed() && !sim.s.is_closed());
    assert!(sim.c.stats.pings >= 15, "pings: {}", sim.c.stats.pings);
    // partition: everything lost from now on -> both sides time out
    sim.loss_pct = 100;
    sim.run_until(Duration::from_secs(40));
    assert!(sim.c.is_closed() && sim.s.is_closed());
    assert!(
        sim.cev.contains(&Event::Closed(Reason::Timeout))
            && sim.sev.contains(&Event::Closed(Reason::Timeout))
    );

    // dead server: handshake retransmits then gives up, with no sockets and no sleeping
    let t0 = Instant::now();
    let mut c = Client::new(Config::default(), t0);
    let mut now = t0;
    while let Some(t) = c.poll_timeout() {
        now = t;
        c.handle_timeout(now);
    }
    assert_eq!(
        c.poll_event(),
        Some(Event::Closed(Reason::RetriesExhausted))
    );
    assert_eq!(c.stats.retransmits, 20);
    assert!(now - t0 >= Duration::from_secs(4)); // 4+ virtual seconds, 0 real ones

    // framing and error handling are plain function calls too
    let mut s = Server::new(Config::default(), t0);
    s.handle_input(t0, b"HEL").unwrap();
    s.handle_input(t0, b"LO x\nREQ 1 hi\n").unwrap(); // a line split across reads, two lines in one read
    assert_eq!(s.poll_transmit().unwrap(), b"WELCOME\n");
    assert_eq!(s.poll_transmit().unwrap(), b"RES 1 HI\n");
    assert!(matches!(
        s.handle_input(t0, b"BOGUS\n"),
        Err(Error::Protocol(_))
    ));
    assert_eq!(s.handle_input(t0, &[b'x'; 2000]), Err(Error::LineTooLong));
    let mut fresh = Client::new(Config::default(), t0);
    assert_eq!(fresh.request(t0, "a\nb"), Err(Error::BadPayload));
}

/// Limits and the closed state. Each block here failed before the matching fix.
fn test_limits() {
    let t0 = Instant::now();
    let cfg = Config::default();

    // 1. a too-long line is refused however it arrives, also when its newline comes in the same chunk
    let mut long = vec![b'x'; Framer::MAX + 1];
    long.push(b'\n');
    let mut f = Framer::default();
    f.push(&long);
    assert_eq!(f.next_line(), Err(Error::LineTooLong), "one chunk");
    let mut f = Framer::default();
    f.push(&long[..Framer::MAX + 1]);
    assert_eq!(f.next_line(), Err(Error::LineTooLong), "split");
    let mut f = Framer::default();
    f.push(&long[1..]); // exactly MAX bytes and a newline
    assert!(matches!(f.next_line(), Ok(Some(line)) if line.len() == Framer::MAX));

    // 2. the client refuses a body whose line the server would refuse
    let mut c = Client::new(cfg, t0);
    assert_eq!(
        c.request(t0, &"x".repeat(Framer::MAX)),
        Err(Error::BadPayload)
    );
    let fits = "x".repeat(Framer::MAX - "REQ 1 ".len());
    assert_eq!(c.request(t0, &fits), Ok(1));

    // 3. a reply is never longer than its request ('\u{149}' grows from 2 to 3 bytes under Unicode uppercasing)
    let mut s = Server::new(cfg, t0);
    s.handle_input(t0, b"HELLO c1\n").unwrap();
    let _welcome = s.poll_transmit();
    let request = format!("REQ 1 {}\n", "\u{149}".repeat(500));
    s.handle_input(t0, request.as_bytes()).unwrap();
    let reply = s.poll_transmit().unwrap();
    assert!(
        reply.len() <= Framer::MAX + 1,
        "reply of {} bytes",
        reply.len()
    );

    // 4. after close: a new request is an error, and late input changes nothing
    let mut c = Client::new(cfg, t0);
    c.request(t0, "hi").unwrap();
    c.handle_input(t0, b"WELCOME\n").unwrap();
    let later = t0 + cfg.idle;
    c.handle_timeout(later);
    assert!(c.is_closed());
    assert_eq!(c.request(later, "more"), Err(Error::Closed));
    while c.poll_event().is_some() {}
    c.handle_input(later, b"RES 1 HI\n").unwrap();
    assert_eq!(c.poll_event(), None, "no response after Closed");
    let mut s = Server::new(cfg, t0);
    s.handle_input(t0, b"HELLO c1\n").unwrap();
    s.handle_timeout(later);
    while s.poll_event().is_some() {}
    while s.poll_transmit().is_some() {}
    s.handle_input(later, b"REQ 1 hi\n").unwrap();
    assert_eq!(
        (s.poll_event(), s.poll_transmit()),
        (None, None),
        "a closed server stays silent"
    );
    println!("limits: line length, payload size, reply size and the closed state hold");
}

// ------------------------------------------------ (b) blocking TcpStream driver
/// The entire "runtime" for a blocking socket. `on_event` returns true when the caller is done.
fn drive_blocking<E: Sans>(
    ep: &mut E,
    stream: &mut TcpStream,
    deadline: Instant,
    mut on_event: impl FnMut(Event) -> bool,
) -> io::Result<()> {
    let mut buf = [0u8; 2048];
    loop {
        while let Some(t) = ep.poll_transmit() {
            stream.write_all(&t)?;
        }
        while let Some(ev) = ep.poll_event() {
            if on_event(ev) {
                return Ok(());
            }
        }
        let now = Instant::now();
        if now >= deadline {
            return Err(io::ErrorKind::TimedOut.into());
        }
        if ep.is_closed() {
            return Ok(());
        }
        let wait = ep.poll_timeout().map_or(Duration::from_millis(500), |t| {
            t.saturating_duration_since(now)
        });
        stream.set_read_timeout(Some(wait.min(deadline - now).max(Duration::from_millis(1))))?;
        match stream.read(&mut buf) {
            Ok(0) => return Ok(()), // peer closed
            Ok(n) => ep
                .handle_input(Instant::now(), &buf[..n])
                .map_err(io::Error::other)?,
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                        | io::ErrorKind::Interrupted
                ) => {}
            Err(e) => return Err(e),
        }
        ep.handle_timeout(Instant::now());
    }
}
fn test_blocking(n: usize) -> Vec<(u32, String)> {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let server = std::thread::spawn(move || {
        let (mut sock, _) = listener.accept().unwrap();
        let mut s = Server::new(Config::default(), Instant::now());
        drive_blocking(&mut s, &mut sock, deadline, |_| false).unwrap();
    });
    let mut sock = TcpStream::connect(addr).unwrap();
    let mut c = Client::new(Config::default(), Instant::now());
    for i in 0..n {
        c.request(Instant::now(), &format!("job {i}")).unwrap();
    }
    let mut got = vec![];
    drive_blocking(&mut c, &mut sock, deadline, |e| {
        if let Event::Response { seq, body } = e {
            got.push((seq, body));
        }
        got.len() == n
    })
    .unwrap();
    drop(sock); // EOF ends the server loop
    server.join().unwrap();
    got.sort();
    got
}

// ------------------------------------------------------ (c) tokio driver
async fn drive_tokio<E: Sans>(
    ep: &mut E,
    stream: &mut tokio::net::TcpStream,
    mut on_event: impl FnMut(Event) -> bool,
) -> io::Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut buf = [0u8; 2048];
    loop {
        while let Some(t) = ep.poll_transmit() {
            stream.write_all(&t).await?;
        }
        while let Some(ev) = ep.poll_event() {
            if on_event(ev) {
                return Ok(());
            }
        }
        if ep.is_closed() {
            return Ok(());
        }
        let wait = ep.poll_timeout().map_or(Duration::from_millis(500), |t| {
            t.saturating_duration_since(Instant::now())
        });
        tokio::select! {
            r = stream.read(&mut buf) => match r? {
                0 => return Ok(()),
                n => ep.handle_input(Instant::now(), &buf[..n]).map_err(io::Error::other)?,
            },
            _ = tokio::time::sleep(wait) => {}
        }
        ep.handle_timeout(Instant::now());
    }
}
async fn test_tokio(n: usize) -> Vec<(u32, String)> {
    let work = async {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut s = Server::new(Config::default(), Instant::now());
            drive_tokio(&mut s, &mut sock, |_| false).await.unwrap();
        });
        let mut sock = tokio::net::TcpStream::connect(addr).await.unwrap();
        let mut c = Client::new(Config::default(), Instant::now());
        for i in 0..n {
            c.request(Instant::now(), &format!("job {i}")).unwrap();
        }
        let mut got = vec![];
        drive_tokio(&mut c, &mut sock, |e| {
            if let Event::Response { seq, body } = e {
                got.push((seq, body));
            }
            got.len() == n
        })
        .await
        .unwrap();
        drop(sock);
        server.await.unwrap();
        got.sort();
        got
    };
    tokio::time::timeout(Duration::from_secs(5), work)
        .await
        .expect("tokio session timed out")
}

fn main() {
    test_simulated();
    test_limits();
    let blocking = test_blocking(5);
    println!("blocking TcpStream: {} responses", blocking.len());
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let tokio_res = rt.block_on(test_tokio(5));
    println!("tokio TcpStream:    {} responses", tokio_res.len());
    // identical logic => identical observable behaviour, whether simulated, blocking or async
    assert_eq!(blocking, expected(5));
    assert_eq!(tokio_res, expected(5));
    assert_eq!(simulate(11, 20, 5).2, expected(5));
    println!("sans-IO: simulator, blocking and tokio drivers agree");
}
