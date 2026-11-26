//! RECIPE 176 (network): a length-prefixed TCP message server, built twice: thread-per-connection (std) and task-per-connection (tokio)
//! LEVEL: advanced
//! Use in: custom RPC / database wire protocols, game servers, brokers, anything speaking over raw TCP.   Run: cargo run --release --example tcp_framed_server
#![allow(dead_code)]
//!
//! WHAT: TCP is a byte STREAM: one `write` of 100 bytes may arrive as 3 reads, and 3 writes may arrive as 1
//! read. So every protocol needs framing. Here: `[len: u32 big-endian][payload]`, with a MAX frame size so a
//! hostile or buggy peer cannot make us allocate 4 GiB. Request payload = `[op][data]` (0 echo, 1 upper-case,
//! 2 length-as-u32); response payload = `[status][data]` (0 ok, 1 error text).
//!
//! DETAILS THAT BITE IN PRODUCTION (all exercised below):
//!  * partial reads: loop until the whole header/payload is in. A read timeout can fire mid-frame, and
//!    `read_exact` discards what it already read on error, so we hand-roll `read_full` that keeps its progress.
//!  * EOF at a frame boundary = polite close (also what a client `shutdown(Write)` half-close looks like: we
//!    still answer everything already sent, then close). EOF mid-frame = protocol error.
//!  * oversized frame: reply with an error and close; the stream cannot be resynchronised after a bad length.
//!  * timeouts: an idle or slowloris client (sends 2 header bytes, then nothing) would pin a thread forever.
//!    Every socket has read AND write timeouts; the idle limit closes quiet peers.
//!  * limits: a connection cap (reject with a "busy" frame instead of accepting unbounded work).
//!  * graceful shutdown: stop accepting (drop the listener), let connections finish what they have received,
//!    then close; the accept loop joins them. Connections notice shutdown only between frames, so in-flight
//!    requests complete (drain).
//!  * `TCP_NODELAY`: request/response ping-pong with Nagle on can stall ~40 ms on delayed ACKs.
//! Everything binds 127.0.0.1:0 (ephemeral port) and never talks outside the machine.
//!
//! THREADS vs TASKS: a thread costs a stack (MBs of virtual memory, a kernel object) and context switches, so
//! it suits dozens to a few thousand connections with simple blocking code. A tokio task costs ~hundreds of
//! bytes and multiplexes thousands of sockets on a few threads via epoll/kqueue (see mio_event_loop), at the price of
//! `async` colouring and cancel-safety care (note we read the first byte with a cancel-safe `read`, not `read_exact`).
//!
//! COMMON USES: Redis-style and Postgres-style wire protocols (length-prefixed messages), gRPC/HTTP2 frames,
//! Kafka's size-prefixed requests, MQTT, internal service RPC, chat/game servers.
//! Read: https://tokio.rs/tokio/tutorial/framing and https://doc.rust-lang.org/std/net/struct.TcpStream.html
use std::io::{self, ErrorKind, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio::time::timeout;

const TICK: Duration = Duration::from_millis(20); // how often idle connections re-check the stop flag
const IO_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Copy)]
struct Cfg {
    max_conns: usize,
    idle: Duration,
    max_frame: usize,
}
impl Cfg {
    const DEFAULT: Cfg = Cfg {
        max_conns: 256,
        idle: Duration::from_secs(5),
        max_frame: 64 * 1024,
    };
}

// ---------- shared protocol logic (no IO) ----------
fn encode_frame(payload: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(4 + payload.len());
    v.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    v.extend_from_slice(payload);
    v
}
fn parse_len(hdr: [u8; 4], max: usize) -> Result<usize, String> {
    let n = u32::from_be_bytes(hdr) as usize;
    if n > max {
        Err(format!("frame of {n} bytes exceeds max {max}"))
    } else {
        Ok(n)
    }
}
fn reply(status: u8, data: &[u8]) -> Vec<u8> {
    let mut p = vec![status];
    p.extend_from_slice(data);
    encode_frame(&p)
}
fn handle_request(req: &[u8]) -> Vec<u8> {
    match req.split_first() {
        Some((0, data)) => reply(0, data),
        Some((1, data)) => reply(0, &data.to_ascii_uppercase()),
        Some((2, data)) => reply(0, &(data.len() as u32).to_be_bytes()),
        Some((op, _)) => reply(1, format!("unknown op {op}").as_bytes()),
        None => reply(1, b"empty request"),
    }
}

struct Handle {
    addr: SocketAddr,
    stop: Arc<AtomicBool>,
    join: Box<dyn FnOnce()>,
}
impl Handle {
    /// Graceful shutdown: stop accepting, drain connections, return when everything has finished.
    fn shutdown(self) {
        self.stop.store(true, Ordering::SeqCst);
        (self.join)();
    }
}

// ---------- model 1: std threads ----------
enum Rd {
    Ok,
    Eof,
    Idle,
    Stop,
}

/// Fill `buf` completely, surviving read timeouts without losing progress.
/// `boundary`: we are between frames (zero bytes read so far means clean EOF / may stop).
fn read_full(
    s: &mut TcpStream,
    buf: &mut [u8],
    boundary: bool,
    idle: Duration,
    stop: &AtomicBool,
) -> io::Result<Rd> {
    let (mut got, mut last) = (0, Instant::now());
    while got < buf.len() {
        match s.read(&mut buf[got..]) {
            Ok(0) if got == 0 && boundary => return Ok(Rd::Eof),
            Ok(0) => return Err(ErrorKind::UnexpectedEof.into()),
            Ok(n) => {
                got += n;
                last = Instant::now();
            }
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                if got == 0 && boundary && stop.load(Ordering::SeqCst) {
                    return Ok(Rd::Stop);
                }
                if last.elapsed() >= idle {
                    return Ok(Rd::Idle);
                }
            }
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(Rd::Ok)
}

fn serve_conn_sync(mut s: TcpStream, cfg: Cfg, stop: &AtomicBool) -> io::Result<()> {
    s.set_nonblocking(false)?; // some platforms inherit O_NONBLOCK from the listener
    s.set_nodelay(true)?;
    s.set_read_timeout(Some(TICK))?;
    s.set_write_timeout(Some(IO_TIMEOUT))?;
    loop {
        let mut hdr = [0u8; 4];
        if !matches!(read_full(&mut s, &mut hdr, true, cfg.idle, stop)?, Rd::Ok) {
            return Ok(()); // EOF, idle timeout or shutdown at a frame boundary
        }
        let len = match parse_len(hdr, cfg.max_frame) {
            Ok(n) => n,
            Err(msg) => {
                s.write_all(&reply(1, msg.as_bytes()))?;
                return Ok(()); // cannot resync after a bad length: close
            }
        };
        let mut payload = vec![0u8; len];
        if !matches!(
            read_full(&mut s, &mut payload, false, cfg.idle, stop)?,
            Rd::Ok
        ) {
            return Ok(()); // slowloris mid-frame: drop
        }
        s.write_all(&handle_request(&payload))?;
    }
}

struct CountGuard(Arc<AtomicUsize>);
impl Drop for CountGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

fn start_threaded(cfg: Cfg) -> Handle {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let stop2 = stop.clone();
    let accept: JoinHandle<()> = thread::spawn(move || {
        let active = Arc::new(AtomicUsize::new(0));
        let mut conns: Vec<JoinHandle<()>> = Vec::new();
        while !stop2.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((mut s, _)) => {
                    if active.load(Ordering::SeqCst) >= cfg.max_conns {
                        let _ = s.set_nonblocking(false);
                        let _ = s.set_write_timeout(Some(Duration::from_secs(1)));
                        let _ = s.write_all(&reply(1, b"busy"));
                        continue; // drop closes
                    }
                    active.fetch_add(1, Ordering::SeqCst); // count BEFORE spawning: no accept race
                    let guard = CountGuard(active.clone());
                    let stop3 = stop2.clone();
                    conns.push(thread::spawn(move || {
                        let _g = guard;
                        let _ = serve_conn_sync(s, cfg, &stop3);
                    }));
                    conns.retain(|h| !h.is_finished());
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(2))
                }
                Err(_) => break,
            }
        }
        drop(listener); // stop accepting: new connects are refused
        for h in conns {
            let _ = h.join(); // drain
        }
    });
    Handle {
        addr,
        stop,
        join: Box::new(move || accept.join().unwrap()),
    }
}

// ---------- model 2: tokio tasks ----------
async fn serve_conn_async(
    mut s: tokio::net::TcpStream,
    cfg: Cfg,
    stop: Arc<AtomicBool>,
) -> io::Result<()> {
    s.set_nodelay(true)?;
    let timed_out = || io::Error::from(ErrorKind::TimedOut);
    let mut idle_since = Instant::now();
    loop {
        // single-byte `read` is cancel-safe, so we can race it against a tick without losing data
        let mut hdr = [0u8; 4];
        match timeout(TICK, s.read(&mut hdr[..1])).await {
            Err(_) => {
                if stop.load(Ordering::SeqCst) || idle_since.elapsed() >= cfg.idle {
                    return Ok(());
                }
                continue;
            }
            Ok(Ok(0)) => return Ok(()), // clean EOF at boundary (client half-close)
            Ok(Ok(_)) => {}
            Ok(Err(e)) => return Err(e),
        }
        timeout(cfg.idle, s.read_exact(&mut hdr[1..]))
            .await
            .map_err(|_| timed_out())??;
        let len = match parse_len(hdr, cfg.max_frame) {
            Ok(n) => n,
            Err(msg) => {
                timeout(IO_TIMEOUT, s.write_all(&reply(1, msg.as_bytes())))
                    .await
                    .map_err(|_| timed_out())??;
                return Ok(());
            }
        };
        let mut payload = vec![0u8; len];
        timeout(cfg.idle, s.read_exact(&mut payload))
            .await
            .map_err(|_| timed_out())??;
        timeout(IO_TIMEOUT, s.write_all(&handle_request(&payload)))
            .await
            .map_err(|_| timed_out())??;
        idle_since = Instant::now();
    }
}

fn start_tokio(rt: &Arc<tokio::runtime::Runtime>, cfg: Cfg) -> Handle {
    let std_l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = std_l.local_addr().unwrap();
    std_l.set_nonblocking(true).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let stop2 = stop.clone();
    let task = rt.spawn(async move {
        let listener = tokio::net::TcpListener::from_std(std_l).unwrap();
        let permits = Arc::new(Semaphore::new(cfg.max_conns));
        let mut set = JoinSet::new();
        while !stop2.load(Ordering::SeqCst) {
            let Ok(Ok((mut s, _))) = timeout(TICK, listener.accept()).await else {
                continue;
            };
            match permits.clone().try_acquire_owned() {
                Ok(permit) => {
                    let stop3 = stop2.clone();
                    set.spawn(async move {
                        let _permit = permit; // released when the task ends
                        let _ = serve_conn_async(s, cfg, stop3).await;
                    });
                }
                Err(_) => {
                    let _ = timeout(Duration::from_secs(1), s.write_all(&reply(1, b"busy"))).await;
                }
            }
            while set.try_join_next().is_some() {} // reap finished tasks
        }
        drop(listener);
        while set.join_next().await.is_some() {} // drain
    });
    let rt2 = rt.clone();
    Handle {
        addr,
        stop,
        join: Box::new(move || rt2.block_on(task).unwrap()),
    }
}

// ---------- client side (blocking, every op has a timeout) ----------
fn connect(addr: SocketAddr) -> TcpStream {
    let s = TcpStream::connect_timeout(&addr, Duration::from_secs(2)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    s.set_write_timeout(Some(Duration::from_secs(5))).unwrap();
    s.set_nodelay(true).unwrap();
    s
}
fn read_reply(s: &mut TcpStream) -> io::Result<(u8, Vec<u8>)> {
    let mut hdr = [0u8; 4];
    s.read_exact(&mut hdr)?; // fine on the client: a timeout is fatal anyway
    let mut p = vec![0u8; u32::from_be_bytes(hdr) as usize];
    s.read_exact(&mut p)?;
    let status = p.remove(0);
    Ok((status, p))
}
fn call(s: &mut TcpStream, op: u8, data: &[u8]) -> io::Result<(u8, Vec<u8>)> {
    let mut req = vec![op];
    req.extend_from_slice(data);
    s.write_all(&encode_frame(&req))?;
    read_reply(s)
}
fn expect_closed(s: &mut TcpStream) -> Duration {
    let t = Instant::now();
    let mut b = [0u8; 1];
    match s.read(&mut b) {
        Ok(0) => {}
        Err(e) if e.kind() == ErrorKind::ConnectionReset => {}
        other => panic!("expected the server to close, got {other:?}"),
    }
    t.elapsed()
}

fn hammer(addr: SocketAddr, clients: usize, msgs: usize, size: usize) {
    let hs: Vec<_> = (0..clients)
        .map(|c| {
            thread::spawn(move || {
                let mut s = connect(addr);
                for m in 0..msgs {
                    let data: Vec<u8> = (0..(m * 7 + c) % size)
                        .map(|i| b'a' + ((i + m + c) % 26) as u8)
                        .collect();
                    let (st, got) = call(&mut s, (m % 2) as u8, &data).unwrap();
                    assert_eq!(st, 0);
                    let want = if m % 2 == 0 {
                        data.clone()
                    } else {
                        data.to_ascii_uppercase()
                    };
                    assert_eq!(got, want, "client {c} msg {m}");
                }
            })
        })
        .collect();
    for h in hs {
        h.join().unwrap();
    }
}

fn suite(label: &str, start: &dyn Fn(Cfg) -> Handle) {
    // 50 clients x 100 messages
    let h = start(Cfg {
        max_frame: 1024,
        ..Cfg::DEFAULT
    });
    hammer(h.addr, 50, 100, 200);
    // exactly max_frame is accepted; one more byte is rejected with an error frame, then close
    let mut s = connect(h.addr);
    let (st, got) = call(&mut s, 0, &vec![b'x'; 1023]).unwrap(); // 1 op byte + 1023 = 1024
    assert_eq!((st, got.len()), (0, 1023));
    s.write_all(&1025u32.to_be_bytes()).unwrap();
    let (st, msg) = read_reply(&mut s).unwrap();
    assert_eq!(st, 1);
    assert!(String::from_utf8_lossy(&msg).contains("exceeds max"));
    expect_closed(&mut s);
    // unknown op: error reply but the connection survives
    let mut s = connect(h.addr);
    assert_eq!(call(&mut s, 9, b"?").unwrap().0, 1);
    assert_eq!(call(&mut s, 2, b"abcd").unwrap(), (0, vec![0, 0, 0, 4]));
    // half-close: send 3 requests, close our write side, still get 3 answers, then EOF
    let mut s = connect(h.addr);
    for i in 0..3u8 {
        s.write_all(&encode_frame(&[0, i])).unwrap();
    }
    s.shutdown(Shutdown::Write).unwrap();
    for i in 0..3u8 {
        assert_eq!(read_reply(&mut s).unwrap(), (0, vec![i]));
    }
    expect_closed(&mut s);
    // a client idle at shutdown is closed by it, and the listener is gone afterwards
    let mut idle_client = connect(h.addr);
    assert_eq!(call(&mut idle_client, 0, b"hi").unwrap().1, b"hi");
    let addr = h.addr;
    h.shutdown();
    expect_closed(&mut idle_client);
    assert!(
        TcpStream::connect_timeout(&addr, Duration::from_secs(1)).is_err(),
        "listener must be closed"
    );

    // slowloris and silent clients are timed out by the idle limit
    let h = start(Cfg {
        idle: Duration::from_millis(200),
        ..Cfg::DEFAULT
    });
    let mut slow = connect(h.addr);
    slow.write_all(&[0, 0]).unwrap(); // half a header, then silence
    let mut silent = connect(h.addr);
    let t1 = expect_closed(&mut slow);
    let t2 = expect_closed(&mut silent);
    assert!(
        t1 >= Duration::from_millis(150) && t1 < Duration::from_secs(3),
        "slowloris dropped after {t1:?}"
    );
    assert!(t2 < Duration::from_secs(3));
    h.shutdown();

    // connection cap: the 3rd concurrent client is told "busy"
    let h = start(Cfg {
        max_conns: 2,
        ..Cfg::DEFAULT
    });
    let mut a = connect(h.addr);
    let mut b = connect(h.addr);
    call(&mut a, 0, b"1").unwrap();
    call(&mut b, 0, b"2").unwrap();
    let mut c = connect(h.addr);
    let (st, msg) = read_reply(&mut c).unwrap();
    assert_eq!((st, &msg[..]), (1, &b"busy"[..]));
    drop((a, b, c));
    h.shutdown();
    println!(
        "[{label}] 50x100 echoes, max frame, half-close, idle timeout, conn cap, graceful shutdown: all ok"
    );
}

fn bench(label: &str, h: Handle) {
    let (clients, msgs) = (16, 2000);
    let t = Instant::now();
    hammer(h.addr, clients, msgs, 128);
    let dt = t.elapsed();
    let n = (clients * msgs) as f64;
    println!(
        "[{label}] {} round trips in {dt:?} = {:.0} req/s",
        clients * msgs,
        n / dt.as_secs_f64()
    );
    h.shutdown();
}

fn main() {
    // watchdog: this recipe must never hang a CI run
    thread::spawn(|| {
        thread::sleep(Duration::from_secs(90));
        eprintln!("watchdog: timed out");
        std::process::exit(2);
    });
    let rt = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap(),
    );

    suite("threads", &start_threaded);
    suite("tokio  ", &|cfg| start_tokio(&rt, cfg));
    bench("threads", start_threaded(Cfg::DEFAULT));
    bench("tokio  ", start_tokio(&rt, Cfg::DEFAULT));
    println!("all framed-server assertions passed");
}
