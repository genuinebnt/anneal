//! RECIPE 177 (network): a single-threaded readiness event loop with `mio`: edge-triggered IO, per-connection state machines, a `Waker`
//! LEVEL: advanced
//! Use in: understanding tokio/async internals, custom high-connection-count servers, proxies, game/realtime servers.   Run: cargo run --release --example mio_event_loop
#![allow(dead_code)]
//!
//! WHAT: one thread serves many sockets. All sockets are NONBLOCKING; the thread sleeps in `Poll::poll`
//! (epoll on Linux, kqueue on macOS/BSD) until the kernel reports READINESS events, then does only the work
//! that cannot block. Registration associates each source with a `Token` (your index into a table).
//!
//! EDGE-TRIGGERED: mio reports a *transition* to ready, once. If you read only part of what is available you
//! will not be told again, so on a readable event you must read until `WouldBlock`, and on the listener you
//! must `accept` until `WouldBlock`. Same for writes: write until `WouldBlock`, then ask for `WRITABLE`
//! interest and resume when the next writable edge arrives.
//!
//! PER-CONNECTION STATE MACHINE: `rbuf` holds an incomplete trailing line (TCP delivers arbitrary fragments);
//! `out` + `wpos` is the write queue (a partial `write` leaves a tail to resume later). WRITABLE interest is
//! toggled on only while `out` is non-empty: a socket that is always writable would otherwise wake us pointlessly.
//! Half-close (peer EOF) is remembered; we flush all pending output before closing, so no complete line goes unanswered (text after
//! the last newline gets no reply).
//!
//! HOSTILE PEERS: both buffers are bounded, so a peer that never sends a newline, or sends and never reads, is disconnected. A
//! connection that was reset before `accept` must not stop the server (on macOS, setting an option on it fails).
//! LIMITS: one peer that keeps its socket readable is read until `WouldBlock` before others get a turn; running out of file
//! descriptors stops the server; a connection that stays open makes shutdown give up after 5 seconds with an error.
//!
//! `mio::Waker`: other threads cannot interrupt `poll` by touching your data, so they `wake()`, which makes the
//! loop return an event with the waker's token (eventfd on Linux, a user event on kqueue). Here it requests
//! graceful shutdown: stop accepting, drain the open connections, then exit.
//!
//! HOW THIS IS TOKIO: tokio's I/O driver is this loop. `tokio::net::TcpStream` registers with a mio `Poll` and
//! keeps a ScheduledIo per source holding the readiness bits and the wakers of tasks waiting on it. A task that
//! does `read().await` tries a nonblocking read; on `WouldBlock` it clears readiness and parks its `Waker`.
//! When the driver's `poll` returns the token, it sets readiness and wakes those tasks, which the scheduler
//! re-polls. `async fn` is the compiler-generated version of the per-connection state machine written by hand
//! below, and `Waker` is the same idea at the task level. Writing this once makes `await` less magical.
//!
//! COMMON USES: the reactor under tokio and other Rust async runtimes; Redis-like single-threaded servers;
//! nginx-style worker loops (one event loop per core); libuv (Node.js) uses the same model.
//! Read: https://docs.rs/mio and https://tokio.rs/tokio/tutorial/io and https://man7.org/linux/man-pages/man7/epoll.7.html
use mio::net::{TcpListener, TcpStream};
use mio::{Events, Interest, Poll, Token, Waker};
use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, ErrorKind, Read, Write};
use std::net::{Shutdown, SocketAddr};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

const WAKER: Token = Token(0);
const LISTENER: Token = Token(1);
/// An unfinished line longer than this ends the connection (checked after each read, so up to one read more is held).
const MAX_LINE: usize = 64 * 1024;
/// So does this much output that the peer has not taken.
const MAX_PENDING: usize = 32 * 1024 * 1024;

#[derive(Default, Debug)]
struct Stats {
    accepted: usize,
    closed: usize,
    lines: usize,
    bytes_in: usize,
    bytes_out: usize,
    write_would_block: usize, // times a write hit a full socket buffer (partial write path)
    max_open: usize,
}

struct Conn {
    stream: TcpStream,
    rbuf: Vec<u8>, // partial line carried between reads
    out: Vec<u8>,  // write queue
    wpos: usize,   // bytes of `out` already written
    peer_closed: bool,
    writable_interest: bool,
}

impl Conn {
    fn pending(&self) -> bool {
        self.wpos < self.out.len()
    }

    /// Edge-triggered: drain the socket until WouldBlock, turning complete lines into queued output.
    fn on_readable(&mut self, st: &mut Stats) -> io::Result<()> {
        let mut buf = [0u8; 16 * 1024];
        loop {
            match self.stream.read(&mut buf) {
                Ok(0) => {
                    self.peer_closed = true; // EOF (peer closed or half-closed its write side)
                    break;
                }
                Ok(n) => {
                    st.bytes_in += n;
                    self.take_lines(&buf[..n], st);
                    if self.rbuf.len() > MAX_LINE {
                        return Err(io::Error::new(ErrorKind::InvalidData, "line too long"));
                    }
                    if self.out.len() - self.wpos > MAX_PENDING {
                        self.flush(st)?; // the peer may be reading: send what it will take first
                        if self.out.len() - self.wpos > MAX_PENDING {
                            return Err(io::Error::other("the peer does not read its replies"));
                        }
                    }
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => break, // drained: now wait for the next edge
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    /// Queue a reply for every complete line; keep the incomplete tail in rbuf.
    fn take_lines(&mut self, new: &[u8], st: &mut Stats) {
        let mut seen = self.rbuf.len(); // no newline before this: do not search it again
        self.rbuf.extend_from_slice(new);
        let mut start = 0;
        while let Some(i) = self.rbuf[seen..].iter().position(|&b| b == b'\n') {
            let end = seen + i;
            let line = &self.rbuf[start..end];
            self.out.extend(line.iter().map(|b| b.to_ascii_uppercase()));
            self.out.push(b'\n');
            st.lines += 1;
            start = end + 1;
            seen = start;
        }
        self.rbuf.drain(..start);
    }

    /// Write until everything is sent or the kernel buffer is full (WouldBlock).
    fn flush(&mut self, st: &mut Stats) -> io::Result<()> {
        while self.pending() {
            match self.stream.write(&self.out[self.wpos..]) {
                Ok(0) => return Err(ErrorKind::WriteZero.into()),
                Ok(n) => {
                    self.wpos += n;
                    st.bytes_out += n;
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => {
                    st.write_would_block += 1;
                    if self.wpos > self.out.len() / 2 {
                        self.out.drain(..self.wpos); // do not keep what has been sent
                        self.wpos = 0;
                    }
                    return Ok(()); // partial write: keep the tail, wait for WRITABLE
                }
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }
        self.out.clear();
        self.wpos = 0;
        Ok(())
    }
}

fn run_server(mut poll: Poll, mut listener: TcpListener) -> io::Result<Stats> {
    poll.registry()
        .register(&mut listener, LISTENER, Interest::READABLE)?;
    let mut events = Events::with_capacity(256);
    let mut conns: HashMap<Token, Conn> = HashMap::new();
    let mut next = 2usize;
    let mut st = Stats::default();
    let mut stopping: Option<Instant> = None;
    let hard_deadline = Instant::now() + Duration::from_secs(30); // safety net: never spin forever

    loop {
        if Instant::now() > hard_deadline {
            return Err(io::Error::new(ErrorKind::TimedOut, "event loop deadline"));
        }
        if stopping.is_some() && conns.is_empty() {
            return Ok(st); // drained
        }
        if stopping.is_some_and(|t| t.elapsed() > Duration::from_secs(5)) {
            return Err(io::Error::new(ErrorKind::TimedOut, "drain timed out"));
        }
        match poll.poll(&mut events, Some(Duration::from_millis(200))) {
            Ok(()) => {}
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
        for ev in events.iter() {
            match ev.token() {
                WAKER => {
                    if stopping.is_none() {
                        stopping = Some(Instant::now());
                        poll.registry().deregister(&mut listener)?; // stop accepting
                    }
                }
                LISTENER => loop {
                    // edge-triggered: accept until WouldBlock or we miss connections
                    match listener.accept() {
                        Ok((mut s, _)) => {
                            if stopping.is_some() {
                                continue; // dropped: refuse late arrivals
                            }
                            // only a latency hint, and it fails on a connection the peer already reset
                            let _ = s.set_nodelay(true);
                            let t = Token(next);
                            next += 1;
                            poll.registry().register(&mut s, t, Interest::READABLE)?;
                            conns.insert(
                                t,
                                Conn {
                                    stream: s,
                                    rbuf: vec![],
                                    out: vec![],
                                    wpos: 0,
                                    peer_closed: false,
                                    writable_interest: false,
                                },
                            );
                            st.accepted += 1;
                            st.max_open = st.max_open.max(conns.len());
                        }
                        Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                        Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                        Err(e) if e.kind() == ErrorKind::ConnectionAborted => continue,
                        Err(e) => return Err(e),
                    }
                },
                tok => {
                    let Some(c) = conns.get_mut(&tok) else {
                        continue;
                    };
                    let mut dead = ev.is_error();
                    if !dead && (ev.is_readable() || ev.is_read_closed()) {
                        dead = c.on_readable(&mut st).is_err();
                    }
                    if !dead {
                        // always try to flush: covers new output (read) and the WRITABLE edge
                        dead = c.flush(&mut st).is_err();
                    }
                    // close only after the peer is done AND everything has been sent
                    if dead || (c.peer_closed && !c.pending()) {
                        poll.registry().deregister(&mut c.stream)?;
                        let _ = c.stream.shutdown(Shutdown::Both);
                        conns.remove(&tok);
                        st.closed += 1;
                    } else if c.pending() != c.writable_interest {
                        // toggle WRITABLE interest only while there is queued output
                        let interest = if c.pending() {
                            Interest::READABLE | Interest::WRITABLE
                        } else {
                            Interest::READABLE
                        };
                        poll.registry().reregister(&mut c.stream, tok, interest)?;
                        c.writable_interest = c.pending();
                    }
                }
            }
        }
    }
}

// ---------- blocking client driver (runs on other threads, all sockets have timeouts) ----------
fn client(addr: SocketAddr) -> std::net::TcpStream {
    let s = std::net::TcpStream::connect_timeout(&addr, Duration::from_secs(2)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    s.set_write_timeout(Some(Duration::from_secs(10))).unwrap();
    s
}

/// Many simultaneous connections, interleaved requests, lines split at awkward byte boundaries.
fn many_clients(addr: SocketAddr, n: usize) {
    let mut conns: Vec<_> = (0..n).map(|_| client(addr)).collect(); // all open at once
    let mut readers: Vec<_> = conns
        .iter()
        .map(|c| BufReader::new(c.try_clone().unwrap()))
        .collect();
    for round in 0..10 {
        for (i, c) in conns.iter_mut().enumerate() {
            let msg = format!("conn {i} round {round}\n");
            // fragment: the server must reassemble across reads
            let (a, b) = msg.as_bytes().split_at(3 + (i + round) % 5);
            c.write_all(a).unwrap();
            c.flush().unwrap();
            c.write_all(b).unwrap();
        }
        for (i, r) in readers.iter_mut().enumerate() {
            let mut line = String::new();
            r.read_line(&mut line).unwrap();
            assert_eq!(line, format!("CONN {i} ROUND {round}\n"));
        }
    }
    // several lines in one write; then half-close and expect EOF after the replies
    for c in conns.iter_mut() {
        c.write_all(b"a\nb\nc\n").unwrap();
        c.shutdown(Shutdown::Write).unwrap();
    }
    for r in readers.iter_mut() {
        let mut rest = String::new();
        r.read_to_string(&mut rest).unwrap(); // returns at EOF = server closed after flushing
        assert_eq!(rest, "A\nB\nC\n");
    }
}

/// Send ~24 MB before reading anything: responses overflow the socket buffers, forcing the
/// partial-write / WRITABLE path in the server. Writer and reader are separate threads (no deadlock).
fn bulk_client(addr: SocketAddr, lines: usize) {
    let s = client(addr);
    let mut w = s.try_clone().unwrap();
    let writer = thread::spawn(move || {
        let mut chunk = Vec::new();
        for i in 0..lines {
            chunk.extend_from_slice(format!("line {i:08} {}\n", "x".repeat(60)).as_bytes());
            if chunk.len() > 256 * 1024 {
                w.write_all(&chunk).unwrap();
                chunk.clear();
            }
        }
        w.write_all(&chunk).unwrap();
        w.shutdown(Shutdown::Write).unwrap();
    });
    thread::sleep(Duration::from_millis(300)); // let the server's output pile up unread
    let mut r = BufReader::new(s);
    let mut line = String::new();
    for i in 0..lines {
        line.clear();
        r.read_line(&mut line).unwrap();
        assert_eq!(line, format!("LINE {i:08} {}\n", "X".repeat(60)));
    }
    line.clear();
    assert_eq!(
        r.read_line(&mut line).unwrap(),
        0,
        "EOF after the last response"
    );
    writer.join().unwrap();
}

/// Close with a reset instead of a normal shutdown.
fn reset(s: std::net::TcpStream) {
    use std::os::fd::AsRawFd;
    let linger = libc::linger {
        l_onoff: 1,
        l_linger: 0,
    };
    // SAFETY: the descriptor is open (we own `s`), and the pointer and length describe `linger`, which outlives the call.
    let rc = unsafe {
        libc::setsockopt(
            s.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_LINGER,
            (&raw const linger).cast(),
            size_of::<libc::linger>() as libc::socklen_t,
        )
    };
    assert_eq!(rc, 0, "setsockopt(SO_LINGER)");
}

fn timed_out(e: &io::Error) -> bool {
    matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut)
}

/// A second server, attacked: it must drop the bad peers, keep serving, and shut down cleanly.
fn hostile_peers() -> io::Result<()> {
    let poll = Poll::new()?;
    let waker = Waker::new(poll.registry(), WAKER)?;
    let listener = TcpListener::bind("127.0.0.1:0".parse().unwrap())?;
    let addr = listener.local_addr()?;
    // reset while still waiting to be accepted: the loop is not running yet
    for _ in 0..20 {
        reset(client(addr));
    }
    let server = thread::spawn(move || run_server(poll, listener));

    // no newline, ever. The write may fail part way; what matters is that the server hangs up.
    let mut endless = client(addr);
    let _ = endless.write_all(&vec![b'x'; 4 * MAX_LINE]);
    match endless.read(&mut [0u8; 1]) {
        Ok(0) => {}
        Ok(_) => panic!("a reply to a line that never ended"),
        Err(e) => assert!(!timed_out(&e), "an endless line was kept: {e}"),
    }

    // sends and never reads: far more than the limit and both socket buffers
    let mut deaf = client(addr);
    let chunk = "y\n".repeat(512 * 1024);
    let refused =
        (0..4 * MAX_PENDING / chunk.len()).find_map(|_| deaf.write_all(chunk.as_bytes()).err());
    assert!(
        refused.is_some_and(|e| !timed_out(&e)),
        "unread replies were buffered without limit"
    );

    let mut good = client(addr);
    good.write_all(b"still here\n")
        .expect("the server is alive");
    let mut line = String::new();
    BufReader::new(good.try_clone()?).read_line(&mut line)?;
    assert_eq!(line, "STILL HERE\n");
    drop((endless, deaf, good));
    waker.wake()?;
    server.join().unwrap()?;
    println!("reset, endless-line and never-reading peers: all dropped, server kept serving");
    Ok(())
}

fn main() -> io::Result<()> {
    thread::spawn(|| {
        thread::sleep(Duration::from_secs(60));
        eprintln!("watchdog: timed out");
        std::process::exit(2);
    });
    let poll = Poll::new()?;
    let waker = Arc::new(Waker::new(poll.registry(), WAKER)?);
    let listener = TcpListener::bind("127.0.0.1:0".parse().unwrap())?; // loopback, ephemeral port
    let addr = listener.local_addr()?;
    println!("event loop listening on {addr}");

    let t0 = Instant::now();
    let server = thread::spawn(move || run_server(poll, listener));

    let lines = 300_000;
    let a = thread::spawn(move || many_clients(addr, 100));
    let b = thread::spawn(move || bulk_client(addr, lines));
    a.join().unwrap();
    b.join().unwrap();

    waker.wake()?; // from another thread: ask the loop to shut down gracefully
    let st = server.join().unwrap()?;
    println!("served in {:?}: {st:?}", t0.elapsed());

    assert_eq!(st.accepted, 101);
    assert_eq!(
        st.closed, 101,
        "every connection closed cleanly (drain finished)"
    );
    assert_eq!(st.lines, 100 * 13 + lines);
    assert_eq!(
        st.bytes_in, st.bytes_out,
        "uppercase echo: same byte count in and out"
    );
    assert!(
        st.max_open > 50,
        "connections were truly concurrent (max_open = {})",
        st.max_open
    );
    assert!(
        st.write_would_block > 0,
        "the partial-write path was exercised"
    );
    hostile_peers()?;
    println!("all mio event-loop assertions passed");
    Ok(())
}
