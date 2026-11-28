//! RECIPE 177 (network): a single-threaded readiness event loop with `mio`: edge-triggered IO, per-connection state machines, a `Waker`
//! LEVEL: advanced
//! Use in: understanding tokio/async internals, custom high-connection-count servers, proxies, game/realtime servers.   Run: cargo run --release --example mio_event_loop
#![allow(dead_code)]
//!
//! One thread serves many sockets. Every socket is non-blocking. The thread sleeps in `Poll::poll` (epoll on Linux,
//! kqueue on macOS and BSD) until the kernel reports that a socket is ready, then does only the work that cannot block.
//! Each registered source gets a `Token`, the key into your own table of connections.
//!
//! Edge-triggered: after an event, mio promises another one only once the operation has returned `WouldBlock`. So on a
//! readable event, read until `WouldBlock`, and on the listener, accept until `WouldBlock`. Writes work the same way:
//! write until `WouldBlock`, then ask for `WRITABLE` interest and carry on at the next writable event.
//!
//! Each connection is a small state machine. `rbuf` holds an unfinished line (TCP delivers bytes in any pieces). `out`
//! and `wpos` are the write queue: a short `write` leaves a tail to send later. `WRITABLE` interest is on only while
//! `out` holds unsent bytes, because a socket is nearly always writable and would wake the loop for nothing.
//! When the peer closes its side, the server sends everything still queued before it closes, so every complete line
//! gets its reply (text after the last newline gets none).
//!
//! Hostile peers: both buffers are bounded, so a peer that never sends a newline, or sends and never reads, is
//! disconnected. A connection reset before `accept` must not stop the server (on macOS, setting an option on it fails).
//! Limits: one peer that keeps its socket readable is read until `WouldBlock` before the others get a turn. Running
//! out of file descriptors stops the server. A connection that stays open makes shutdown give up after 5 seconds.
//!
//! `mio::Waker`: another thread can't interrupt `poll` by changing your data, so it calls `wake()`. The loop then gets
//! an event with the waker's token (an eventfd on Linux, a user event on kqueue). Here it asks for a graceful
//! shutdown: stop accepting, finish the open connections, then return.
//!
//! In Tokio: the I/O driver is this loop. Each registered socket has a `ScheduledIo` with its readiness bits and the
//! wakers of the tasks waiting on it. A task's `read().await` tries a non-blocking read. On `WouldBlock`, it clears
//! the readiness and leaves its waker. When `poll` reports the token, the driver sets the readiness and wakes those
//! tasks. An `async fn` is the compiler-made version of the per-connection state machine written by hand below.
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

/// What an event says about a connection. mio calls error and close readiness hints, so they decide nothing alone.
#[derive(Clone, Copy)]
struct Hints {
    readable: bool,
    read_closed: bool,
    error: bool,
}

impl Conn {
    fn new(stream: TcpStream) -> Conn {
        Conn {
            stream,
            rbuf: vec![],
            out: vec![],
            wpos: 0,
            peer_closed: false,
            writable_interest: false,
        }
    }

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

    /// Handles one event; returns true if the connection is dead. The read itself reports errors and the end of input.
    fn on_event(&mut self, hints: Hints, st: &mut Stats) -> bool {
        if (hints.readable || hints.read_closed || hints.error) && self.on_readable(st).is_err() {
            return true;
        }
        // always try to flush: covers new output (read) and the WRITABLE edge
        self.flush(st).is_err()
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
                            conns.insert(t, Conn::new(s));
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
                    let hints = Hints {
                        readable: ev.is_readable(),
                        read_closed: ev.is_read_closed(),
                        error: ev.is_error(),
                    };
                    let dead = c.on_event(hints, &mut st);
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

/// An error hint on a healthy connection must not close it: the connection is read, and the reply goes out.
fn error_hint_is_only_a_hint() -> io::Result<()> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let mut peer = client(listener.local_addr()?);
    let (server_side, _) = listener.accept()?;
    peer.write_all(b"hello\n")?;
    server_side.peek(&mut [0u8; 1])?; // blocks until the line has arrived
    server_side.set_nonblocking(true)?;
    let mut conn = Conn::new(TcpStream::from_std(server_side));
    let mut st = Stats::default();
    let hint = Hints {
        readable: false,
        read_closed: false,
        error: true,
    };
    assert!(!conn.on_event(hint, &mut st), "a hint alone closed a healthy connection");
    let mut line = String::new();
    BufReader::new(peer).read_line(&mut line)?;
    assert_eq!((line.as_str(), st.lines), ("HELLO\n", 1));
    Ok(())
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
    error_hint_is_only_a_hint()?;
    println!("all mio event-loop assertions passed");
    Ok(())
}
