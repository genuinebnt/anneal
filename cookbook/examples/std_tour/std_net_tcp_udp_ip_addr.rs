//! RECIPE 1065 (std_tour): `std::net` by use case: IP and socket addresses, CIDR checks, blocking TCP and UDP on loopback, framing, timeouts, shutdown.
//! LEVEL: intermediate
//! Use in: health checks, small internal services, test servers, allow/deny lists, SSRF filters, understanding what async runtimes wrap.   Run: cargo run --release --example std_net_tcp_udp_ip_addr
//! COMMON USES: parse and validate configured addresses; bind port 0 in tests; length-prefixed or line protocols; read timeouts; half-close; stopping an accept loop.
//! METHODS: IpAddr, Ipv4Addr::new, Ipv4Addr::from, Ipv4Addr::octets, Ipv4Addr::to_bits, Ipv4Addr::from_bits, Ipv4Addr::is_loopback, Ipv4Addr::is_private,
//! Ipv4Addr::is_link_local, Ipv4Addr::is_multicast, Ipv4Addr::is_broadcast, Ipv4Addr::is_unspecified, Ipv4Addr::to_ipv6_mapped, Ipv6Addr::segments,
//! Ipv6Addr::to_ipv4_mapped, Ipv6Addr::to_canonical, IpAddr::to_canonical, Ipv6Addr::is_unique_local, Ipv6Addr::is_unicast_link_local,
//! SocketAddr::new, SocketAddr::from, SocketAddr::ip, SocketAddr::port, SocketAddr::set_port, SocketAddr::is_ipv4, ToSocketAddrs::to_socket_addrs,
//! TcpListener::bind, TcpListener::local_addr, TcpListener::accept, TcpListener::incoming, TcpListener::set_nonblocking, TcpStream::connect,
//! TcpStream::connect_timeout, TcpStream::peer_addr, TcpStream::local_addr, TcpStream::set_read_timeout, TcpStream::set_write_timeout,
//! TcpStream::read_timeout, TcpStream::set_nodelay, TcpStream::nodelay, TcpStream::set_ttl, TcpStream::ttl, TcpStream::peek, TcpStream::shutdown,
//! TcpStream::try_clone, TcpStream::set_nonblocking, TcpStream::take_error, UdpSocket::bind, UdpSocket::send_to, UdpSocket::recv_from,
//! UdpSocket::connect, UdpSocket::send, UdpSocket::recv, UdpSocket::peek_from, UdpSocket::set_read_timeout, UdpSocket::set_broadcast, UdpSocket::broadcast
//! DOCS: https://doc.rust-lang.org/std/net/index.html
//! DEFINED IN: core: the address types `IpAddr`, `Ipv4Addr`, `Ipv6Addr`, `SocketAddr`, `SocketAddrV4/V6`, `AddrParseError` (`core::net`, re-exported by std).
//!   std: `TcpListener`, `TcpStream`, `UdpSocket`, `ToSocketAddrs`, `Shutdown` (they need an OS). std has no TLS, HTTP or async networking.
//! NOT COVERED (and why): `is_global`, `is_documentation` for IPv6, `is_shared`, `set_linger`, `set_quickack` (unstable or Linux only); multicast joins (needs a
//! multicast-capable interface); DNS lookups and any non-loopback traffic (this file never leaves 127.0.0.1/::1); socket options std lacks (use `socket2`).
//! Beyond `tcp_framed_server` (one framed server): this file is the API look-up page. Checked on macOS only.
//!
//! CHEAT SHEET (I want to ... -> use ...)
//!   parse "1.2.3.4:80" / "[::1]:80"           -> `s.parse::<SocketAddr>()` (never resolves names)
//!   accept "localhost:80" or ("host", 80)     -> `to_socket_addrs()?` (may do blocking DNS; may return several addresses: try each)
//!   a free port in a test                     -> bind "127.0.0.1:0", then `local_addr()`
//!   avoid hanging on a silent peer            -> `set_read_timeout(Some(d))` and `set_write_timeout`; connect: `connect_timeout`
//!   recognise a timeout portably              -> `matches!(e.kind(), WouldBlock | TimedOut)` (unix and Windows differ)
//!   tell the peer "I am done sending"         -> `shutdown(Shutdown::Write)`; you can still read the reply
//!   read and write from two threads           -> `try_clone()` (same socket: shutdown and timeouts affect both)
//!   read one whole message                    -> frame it: length prefix + `read_exact`, or delimiter + `BufReader`; ALWAYS cap the size
//!   small request/response latency            -> one `write_all` per message and/or `set_nodelay(true)`
//!   stop a blocking accept loop               -> set a flag, then connect to yourself to wake it; or non-blocking accept + poll
//!   block internal targets (SSRF)             -> call `to_canonical()` first, then check every range on the resolved IP you will connect to
//!
//! PITFALLS: TCP is a byte stream: one `write` is not one `read`. `set_read_timeout(Some(ZERO))` is an error. A UDP datagram longer than the buffer is
//! truncated (some platforms report an error instead). On Apple/BSD an accepted socket inherits the listener's non-blocking flag (Linux does not):
//! always set the mode you want on the accepted stream.
//! TRY NEXT:
//!   - [S] In section 4 remove `shutdown(Shutdown::Write)`. Predict which side blocks in `read_to_end`, and which timeout ends the test.
//!   - [S] Add IPv6 CIDR support to `Cidr` using `u128` and test `fd00::/8`.
//!   - [M] Add a per-frame read deadline: a client that sends one byte per timeout interval must still be cut off (a per-read timeout alone does not do it).
//!   - [M] Add retransmission with sequence numbers to the UDP client and drop every second reply in the server to test it.
//!   - [A] PROJECT: combine with `std_sync_primitives_use_cases` and `std_thread_scope_local_park`: a thread-pool server with a connection limit, load shedding and graceful drain.
//! Read: https://doc.rust-lang.org/std/net/struct.TcpStream.html ; https://doc.rust-lang.org/std/net/struct.UdpSocket.html ; OWASP SSRF cheat sheet https://cheatsheetseries.owasp.org/cheatsheets/Server_Side_Request_Forgery_Prevention_Cheat_Sheet.html
#![allow(dead_code)]

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{
    IpAddr, Ipv4Addr, Ipv6Addr, Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs,
    UdpSocket,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

/// Safety net so that a bug fails the run instead of hanging it.
const GUARD: Duration = Duration::from_secs(20);

fn guarded(s: TcpStream) -> TcpStream {
    s.set_read_timeout(Some(GUARD)).unwrap();
    s.set_write_timeout(Some(GUARD)).unwrap();
    s
}
fn listen() -> (TcpListener, SocketAddr) {
    let l = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let a = l.local_addr().unwrap();
    (l, a)
}
fn connect(a: SocketAddr) -> TcpStream {
    guarded(TcpStream::connect_timeout(&a, GUARD).expect("connect loopback"))
}
fn is_timeout(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
    )
}

// ------------------------------------------------------------ 1. Addresses
fn s01_addresses() {
    let a: SocketAddr = "10.1.2.3:8080".parse().unwrap();
    assert_eq!(
        (a.ip(), a.port(), a.is_ipv4()),
        (IpAddr::V4(Ipv4Addr::new(10, 1, 2, 3)), 8080, true)
    );
    let b: SocketAddr = "[2001:db8::1]:443".parse().unwrap();
    assert!(b.is_ipv6());
    assert_eq!(b.to_string(), "[2001:db8::1]:443"); // IPv6 needs brackets in socket addresses
    assert_eq!(
        SocketAddr::from(([127, 0, 0, 1], 80)),
        SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 80)
    );
    let mut c = a;
    c.set_port(1);
    assert_eq!(c.to_string(), "10.1.2.3:1");
    // strict parsing: names, missing ports, out-of-range and leading zeros are rejected
    for bad in [
        "localhost:80",
        "10.1.2.3",
        "10.1.2.3:65536",
        "256.0.0.1:1",
        "010.0.0.1:1",
        "::1:80",
        " 1.2.3.4:5",
    ] {
        assert!(bad.parse::<SocketAddr>().is_err(), "{bad}");
    }
    assert!(
        "010.0.0.1".parse::<Ipv4Addr>().is_err(),
        "no octal ambiguity"
    );
    assert!("127.1".parse::<Ipv4Addr>().is_err(), "no shorthand");
    assert_eq!(
        "::ffff:1.2.3.4".parse::<IpAddr>().map(|i| i.is_ipv6()),
        Ok(true)
    );

    let v4 = Ipv4Addr::from([192, 168, 1, 10]);
    assert_eq!(v4.octets(), [192, 168, 1, 10]);
    assert_eq!(v4.to_bits(), 0xC0A8_010A); // 1.80; big-endian numeric value, so ordering matches address order
    assert_eq!(Ipv4Addr::from_bits(0xC0A8_010A), v4);
    assert_eq!(u32::from(v4), v4.to_bits());

    let ip = |s: &str| s.parse::<Ipv4Addr>().unwrap();
    assert!(ip("127.255.0.1").is_loopback(), "all of 127.0.0.0/8");
    assert!(
        ["10.0.0.1", "172.16.0.1", "172.31.255.255", "192.168.0.1"]
            .iter()
            .all(|s| ip(s).is_private())
    );
    assert!(
        !ip("172.32.0.1").is_private() && !ip("100.64.0.1").is_private(),
        "carrier-grade NAT is not 'private' here"
    );
    assert!(
        ip("169.254.169.254").is_link_local(),
        "the usual cloud metadata address"
    );
    assert!(ip("224.0.0.1").is_multicast() && ip("239.255.255.255").is_multicast());
    assert!(ip("255.255.255.255").is_broadcast() && ip("0.0.0.0").is_unspecified());

    let v6: Ipv6Addr = "fd12:3456::1".parse().unwrap();
    assert_eq!(v6.segments()[..2], [0xfd12, 0x3456]);
    assert!(
        v6.is_unique_local()
            && "fe80::1"
                .parse::<Ipv6Addr>()
                .unwrap()
                .is_unicast_link_local()
    ); // 1.84
    assert!(Ipv6Addr::LOCALHOST.is_loopback() && Ipv6Addr::UNSPECIFIED.is_unspecified());

    // SECURITY PITFALL: ::ffff:127.0.0.1 reaches IPv4 loopback, but as an Ipv6Addr it is NOT "loopback"
    let sneaky: IpAddr = "::ffff:127.0.0.1".parse().unwrap();
    assert!(!sneaky.is_loopback());
    assert!(sneaky.to_canonical().is_loopback()); // 1.75: unwrap IPv4-mapped addresses before any policy check
    assert_eq!(
        ip("1.2.3.4").to_ipv6_mapped().to_ipv4_mapped(),
        Some(ip("1.2.3.4"))
    );
    assert_eq!(Ipv6Addr::LOCALHOST.to_ipv4_mapped(), None);
    assert_eq!(
        Ipv6Addr::LOCALHOST.to_canonical(),
        IpAddr::V6(Ipv6Addr::LOCALHOST)
    );
    println!("1. addresses ok");
}

// ------------------------------------------------------------ 2. CIDR blocks and an egress policy
#[derive(Debug, Clone, Copy)]
struct Cidr {
    net: u32,
    mask: u32,
}
impl Cidr {
    fn parse(s: &str) -> Option<Cidr> {
        let (ip, len) = s.split_once('/')?;
        let len: u32 = len.parse().ok().filter(|l| *l <= 32)?;
        // `u32::MAX << 32` overflows, so /0 needs checked_shl
        let mask = u32::MAX.checked_shl(32 - len).unwrap_or(0);
        Some(Cidr {
            net: ip.parse::<Ipv4Addr>().ok()?.to_bits() & mask,
            mask,
        })
    }
    fn contains(&self, ip: Ipv4Addr) -> bool {
        ip.to_bits() & self.mask == self.net
    }
}

/// A deliberately conservative example policy, not a complete list of special ranges. Anything unknown in IPv6 is refused.
fn allowed_target(ip: IpAddr) -> bool {
    match ip.to_canonical() {
        IpAddr::V4(v) => {
            let extra = [
                "0.0.0.0/8",
                "100.64.0.0/10",
                "192.0.0.0/24",
                "198.18.0.0/15",
                "240.0.0.0/4",
            ];
            !(v.is_loopback()
                || v.is_private()
                || v.is_link_local()
                || v.is_multicast()
                || v.is_broadcast()
                || v.is_unspecified())
                && !extra.iter().any(|c| Cidr::parse(c).unwrap().contains(v))
        }
        IpAddr::V6(v) => {
            let [first, second, ..] = v.segments();
            // Global unicast 2000::/3 only, and not the tunnels inside it: 6to4 (2002::/16) and Teredo
            // (in 2001::/23) carry an IPv4 address, so they can name a private one.
            (first & 0xe000) == 0x2000 && first != 0x2002 && !(first == 0x2001 && second < 0x0200)
        }
    }
}

fn s02_cidr() {
    let c = Cidr::parse("10.20.0.0/14").unwrap();
    assert!(
        c.contains(Ipv4Addr::new(10, 23, 255, 255)) && !c.contains(Ipv4Addr::new(10, 24, 0, 0))
    );
    assert!(
        Cidr::parse("0.0.0.0/0")
            .unwrap()
            .contains(Ipv4Addr::new(8, 8, 8, 8))
    );
    assert!(
        Cidr::parse("1.2.3.4/32")
            .unwrap()
            .contains(Ipv4Addr::new(1, 2, 3, 4))
    );
    assert!(Cidr::parse("1.2.3.4/33").is_none() && Cidr::parse("1.2.3.4").is_none());
    for bad in [
        "127.0.0.1",
        "10.0.0.5",
        "169.254.169.254",
        "0.0.0.0",
        "100.64.1.1",
        "::1",
        "::ffff:10.0.0.1",
        "fd00::1",
        "fe80::1",
        "192.0.0.192",
        "2002:7f00:1::",    // 6to4 form of 127.0.0.1
        "2002:a9fe:a9fe::", // 6to4 form of 169.254.169.254
        "2001:0:1::",       // Teredo
    ] {
        assert!(!allowed_target(bad.parse().unwrap()), "{bad}");
    }
    assert!(
        allowed_target("93.184.216.34".parse().unwrap())
            && allowed_target("2606:4700::1111".parse().unwrap())
    );

    // ToSocketAddrs on numeric input needs no DNS
    let v: Vec<SocketAddr> = "127.0.0.1:0".to_socket_addrs().unwrap().collect();
    assert_eq!(v, ["127.0.0.1:0".parse().unwrap()]);
    assert_eq!(
        ("::1", 9000)
            .to_socket_addrs()
            .unwrap()
            .next()
            .unwrap()
            .to_string(),
        "[::1]:9000"
    );
    assert!("no-port".to_socket_addrs().is_err());
    println!("2. cidr / policy ok");
}

// ------------------------------------------------------------ 3. TCP: listener, stream options
fn s03_tcp_options() {
    let (listener, addr) = listen();
    assert!(addr.port() != 0, "port 0 was replaced by a real one");
    assert_eq!(
        TcpListener::bind(addr).unwrap_err().kind(),
        io::ErrorKind::AddrInUse
    );

    let client = connect(addr); // succeeds before accept: the kernel completes the handshake into the backlog
    let (server, peer) = listener.accept().unwrap();
    assert_eq!(peer, client.local_addr().unwrap());
    assert_eq!(server.peer_addr().unwrap(), peer);
    assert_eq!(client.peer_addr().unwrap(), addr);

    client.set_nodelay(true).unwrap();
    assert!(client.nodelay().unwrap());
    client.set_ttl(42).unwrap();
    assert_eq!(client.ttl().unwrap(), 42);
    assert_eq!(
        client
            .set_read_timeout(Some(Duration::ZERO))
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert!(client.read_timeout().unwrap().is_some());
    assert!(client.take_error().unwrap().is_none());

    // a silent peer: the read times out and the connection stays usable
    let mut client = client;
    let mut server = guarded(server);
    client
        .set_read_timeout(Some(Duration::from_millis(20)))
        .unwrap();
    let mut buf = [0u8; 16];
    let err = client.read(&mut buf).unwrap_err();
    assert!(is_timeout(&err), "{err:?}");
    server.write_all(b"late").unwrap();
    client.set_read_timeout(Some(GUARD)).unwrap();
    // peek returns data without consuming it
    let mut seen = [0u8; 4];
    while client.peek(&mut seen).unwrap() < 4 {
        thread::yield_now();
    }
    let mut got = [0u8; 4];
    client.read_exact(&mut got).unwrap();
    assert_eq!((&seen, &got), (b"late", b"late"));

    // a write timeout: the peer never reads, so the kernel buffers fill and the write must give up
    client
        .set_write_timeout(Some(Duration::from_millis(50)))
        .unwrap();
    let block = vec![0u8; 64 * 1024];
    let mut sent = 0usize;
    let err = loop {
        match client.write(&block) {
            Ok(n) => sent += n,
            Err(e) => break e,
        }
        assert!(sent < 1 << 30, "kernel buffers cannot be this large");
    };
    assert!(is_timeout(&err), "{err:?}");
    // NOTE: after a timed-out write_all you cannot know how much was sent, so the stream framing is lost: close the connection.

    // nobody listening
    let (l, dead) = listen();
    drop(l);
    assert_eq!(
        TcpStream::connect_timeout(&dead, GUARD).unwrap_err().kind(),
        io::ErrorKind::ConnectionRefused
    );
    assert!(
        TcpStream::connect_timeout(&dead, Duration::ZERO).is_err(),
        "zero timeout is an error"
    );
    // `connect` accepts several candidates and returns the first success
    let (l, live) = listen();
    let s = TcpStream::connect(&[dead, live][..]).unwrap();
    assert_eq!(s.peer_addr().unwrap(), live);
    drop(l);
    println!("3. tcp options / timeouts ok ({sent} bytes buffered before the write blocked)");
}

// ------------------------------------------------------------ 4. Echo, half-close, try_clone
fn s04_echo() {
    let (listener, addr) = listen();
    let server = thread::spawn(move || {
        let mut handlers = Vec::new();
        // incoming() is an endless iterator of accept(); this server stops after a known number of connections
        for conn in listener.incoming().take(4) {
            let conn = guarded(conn.unwrap());
            handlers.push(thread::spawn(move || {
                let mut reader = conn.try_clone().unwrap();
                let mut writer = conn;
                let n = io::copy(&mut reader, &mut writer).unwrap(); // echo until the client's EOF
                writer.shutdown(Shutdown::Write).unwrap();
                n
            }));
        }
        handlers.into_iter().map(|h| h.join().unwrap()).sum::<u64>()
    });
    let totals: u64 = thread::scope(|s| {
        let hs: Vec<_> = (0..4u8)
            .map(|id| {
                s.spawn(move || {
                    let payload = vec![id; 300_000 + id as usize]; // larger than socket buffers
                    let mut conn = connect(addr);
                    let mut tx = conn.try_clone().unwrap();
                    // write and read concurrently, or both sides block with full buffers
                    let echoed = thread::scope(|s2| {
                        s2.spawn(|| {
                            tx.write_all(&payload).unwrap();
                            tx.shutdown(Shutdown::Write).unwrap(); // half-close: the server sees EOF; we can still read
                        });
                        let mut back = Vec::new();
                        conn.read_to_end(&mut back).unwrap(); // ends only because the server also closes its side
                        back
                    });
                    assert_eq!(echoed, payload);
                    payload.len() as u64
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).sum()
    });
    assert_eq!(server.join().unwrap(), totals);
    println!("4. echo with half-close ok");
}

// ------------------------------------------------------------ 5. Framing over a byte stream
const MAX_FRAME: u32 = 64 * 1024;

fn write_frame<W: Write>(w: &mut W, body: &[u8]) -> io::Result<()> {
    let len = u32::try_from(body.len())
        .ok()
        .filter(|l| *l <= MAX_FRAME)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "frame too big"))?;
    let mut msg = Vec::with_capacity(4 + body.len());
    msg.extend_from_slice(&len.to_be_bytes());
    msg.extend_from_slice(body);
    w.write_all(&msg) // header and body in ONE write: avoids the small-write/delayed-ACK stall
}

/// Ok(None) = clean EOF between frames. EOF inside a frame is an error.
fn read_frame<R: Read>(r: &mut R) -> io::Result<Option<Vec<u8>>> {
    let mut head = [0u8; 4];
    let mut got = 0;
    while got < 4 {
        match r.read(&mut head[got..]) {
            Ok(0) if got == 0 => return Ok(None),
            Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
            Ok(n) => got += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    let len = u32::from_be_bytes(head);
    if len > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("frame of {len} bytes refused"),
        )); // before allocating
    }
    let mut body = vec![0; len as usize];
    r.read_exact(&mut body)?;
    Ok(Some(body))
}

/// A line with at most `max` bytes of content, not counting its "\n" or "\r\n". `read_line` alone would
/// buffer an endless line until memory runs out. An endless line is refused once `max + 2` bytes are in.
fn read_bounded_line<R: BufRead>(r: &mut R, max: u64) -> io::Result<Option<String>> {
    let bad = |why| io::Error::new(io::ErrorKind::InvalidData, why);
    let mut line = String::new();
    // room for the content and a two-byte ending
    let n = r.take(max.saturating_add(2)).read_line(&mut line)?;
    if n == 0 {
        return Ok(None);
    }
    let Some(text) = line.strip_suffix('\n') else {
        return Err(bad(if n as u64 > max {
            "line too long"
        } else {
            "connection closed mid-line"
        }));
    };
    let text = text.strip_suffix('\r').unwrap_or(text);
    if text.len() as u64 > max {
        return Err(bad("line too long"));
    }
    let len = text.len();
    line.truncate(len);
    Ok(Some(line))
}

fn s05_framing() {
    let (listener, addr) = listen();
    let sizes = [0usize, 1, 5, 1_000, MAX_FRAME as usize];
    let server = thread::spawn(move || {
        let mut s = guarded(listener.accept().unwrap().0);
        let mut seen = Vec::new();
        while let Some(f) = read_frame(&mut s).unwrap() {
            seen.push(f.len());
            write_frame(&mut s, &f).unwrap();
        }
        seen
    });
    let mut c = connect(addr);
    for n in sizes {
        let body: Vec<u8> = (0..n).map(|i| i as u8).collect();
        write_frame(&mut c, &body).unwrap();
        assert_eq!(read_frame(&mut c).unwrap(), Some(body));
    }
    assert_eq!(
        write_frame(&mut c, &vec![0; MAX_FRAME as usize + 1])
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    // dribble one frame a byte at a time: the reader must reassemble it regardless of packet boundaries
    c.set_nodelay(true).unwrap();
    for byte in [0, 0, 0, 3, b'a', b'b', b'c'] {
        c.write_all(&[byte]).unwrap();
        thread::yield_now();
    }
    assert_eq!(
        read_frame(&mut c).unwrap().as_deref(),
        Some(b"abc".as_slice())
    );
    c.shutdown(Shutdown::Write).unwrap();
    assert_eq!(
        server.join().unwrap(),
        [0, 1, 5, 1_000, MAX_FRAME as usize, 3]
    );

    // the limit counts content only, whichever line ending is used
    for text in [&b"abcd\n"[..], b"abcd\r\n"] {
        let line = read_bounded_line(&mut &text[..], 4).unwrap();
        assert_eq!(line.as_deref(), Some("abcd"));
    }
    for text in [&b"abcde\n"[..], b"abcde\r\n", b"abcdef"] {
        assert!(read_bounded_line(&mut &text[..], 4).is_err());
    }

    // the decoder is generic, so hostile inputs are tested without sockets
    assert_eq!(
        read_frame(&mut &u32::MAX.to_be_bytes()[..])
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
    assert_eq!(
        read_frame(&mut &[0u8, 0][..]).unwrap_err().kind(),
        io::ErrorKind::UnexpectedEof
    );
    assert_eq!(
        read_frame(&mut &[0u8, 0, 0, 9, 1][..]).unwrap_err().kind(),
        io::ErrorKind::UnexpectedEof
    );
    assert_eq!(read_frame(&mut io::empty()).unwrap(), None);

    // line protocol over TCP
    let (listener, addr) = listen();
    let server = thread::spawn(move || {
        let conn = guarded(listener.accept().unwrap().0);
        let mut out = conn.try_clone().unwrap();
        let mut input = BufReader::new(conn); // keep ONE BufReader per connection: dropping it loses buffered bytes
        loop {
            match read_bounded_line(&mut input, 32) {
                Ok(Some(cmd)) => {
                    let reply = match cmd.split_once(' ') {
                        Some(("ECHO", rest)) => format!("+{rest}\n"),
                        _ if cmd == "PING" => "+PONG\n".to_string(),
                        _ => "-ERR unknown\n".to_string(),
                    };
                    out.write_all(reply.as_bytes()).unwrap();
                }
                Ok(None) => return Ok(()),
                Err(e) => {
                    out.write_all(b"-ERR protocol\n").unwrap();
                    return Err(e.to_string());
                }
            }
        }
    });
    let mut c = connect(addr);
    c.write_all(b"PING\nECHO hi there\r\nNOPE\n").unwrap(); // pipelined: three requests in one packet
    let mut replies = BufReader::new(c.try_clone().unwrap()).lines();
    for want in ["+PONG", "+hi there", "-ERR unknown"] {
        assert_eq!(replies.next().unwrap().unwrap(), want);
    }
    c.write_all(&[b'A'; 34]).unwrap(); // no newline, and no room left even for "\r\n"
    assert_eq!(replies.next().unwrap().unwrap(), "-ERR protocol");
    assert_eq!(server.join().unwrap(), Err("line too long".to_string()));
    println!("5. framing ok");
}

// ------------------------------------------------------------ 6. Graceful shutdown of accept loops
fn serve_until_stopped(listener: TcpListener, stop: Arc<AtomicBool>) -> thread::JoinHandle<usize> {
    thread::spawn(move || {
        let mut served = 0;
        thread::scope(|s| {
            for conn in listener.incoming() {
                if stop.load(Ordering::Acquire) {
                    break; // the wake-up connection lands here
                }
                // A failed accept (out of file descriptors, say) must not end the server. Nor may a client
                // that has already reset the connection, which can make setting a timeout fail.
                let Ok(mut conn) = conn else {
                    thread::sleep(Duration::from_millis(10));
                    continue;
                };
                if conn.set_write_timeout(Some(GUARD)).is_err() {
                    continue;
                }
                served += 1;
                s.spawn(move || {
                    let _ = conn.write_all(b"hello");
                });
            }
        }); // scope: in-flight handlers finish before we return (drain)
        served
    })
}

fn s06_shutdown() {
    // (a) flag + self-connect
    let (listener, addr) = listen();
    let stop = Arc::new(AtomicBool::new(false));
    let h = serve_until_stopped(listener, Arc::clone(&stop));
    for _ in 0..3 {
        let mut s = String::new();
        connect(addr).read_to_string(&mut s).unwrap();
        assert_eq!(s, "hello");
    }
    stop.store(true, Ordering::Release);
    drop(connect(addr)); // unblocks accept
    assert_eq!(h.join().unwrap(), 3);

    // (b) non-blocking accept + poll
    let (listener, addr) = listen();
    listener.set_nonblocking(true).unwrap();
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    let mut client = connect(addr);
    let deadline = Instant::now() + GUARD;
    let accepted = loop {
        match listener.accept() {
            Ok((s, _)) => break s,
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                assert!(Instant::now() < deadline);
                thread::sleep(Duration::from_millis(1)); // real servers use poll/epoll/kqueue (mio, tokio) instead of sleeping
            }
            Err(e) => panic!("{e}"),
        }
    };
    // Apple/BSD: the accepted socket inherited O_NONBLOCK, so a read with no data returns immediately.
    #[cfg(target_vendor = "apple")]
    {
        let mut a = &accepted;
        assert_eq!(
            a.read(&mut [0u8; 1]).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
    }
    accepted.set_nonblocking(false).unwrap(); // portable: set the mode you need explicitly
    let mut accepted = guarded(accepted);
    client.write_all(b"x").unwrap();
    let mut one = [0u8; 1];
    accepted.read_exact(&mut one).unwrap();
    assert_eq!(&one, b"x");

    // shutdown(Both) from another handle unblocks a thread that is stuck in read
    let reader = accepted.try_clone().unwrap();
    let t = thread::spawn(move || {
        let mut r = reader;
        r.read(&mut [0u8; 8]).unwrap_or(0) // either EOF (Ok(0)) or an error, never data
    });
    accepted.shutdown(Shutdown::Both).unwrap();
    assert_eq!(t.join().unwrap(), 0);
    println!("6. graceful shutdown ok");
}

// ------------------------------------------------------------ 7. UDP
fn udp() -> (UdpSocket, SocketAddr) {
    let s = UdpSocket::bind("127.0.0.1:0").unwrap();
    s.set_read_timeout(Some(GUARD)).unwrap();
    let a = s.local_addr().unwrap();
    (s, a)
}

/// Request/response with retries. UDP gives no delivery guarantee, so the client owns timeouts and retransmission.
fn udp_call(
    sock: &UdpSocket,
    request: &[u8],
    attempts: u32,
    wait: Duration,
) -> io::Result<(Vec<u8>, u32)> {
    sock.set_read_timeout(Some(wait))?;
    let mut buf = [0u8; 1500];
    for attempt in 1..=attempts {
        sock.send(request)?;
        match sock.recv(&mut buf) {
            Ok(n) => return Ok((buf[..n].to_vec(), attempt)),
            Err(e) if is_timeout(&e) => continue,
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::new(io::ErrorKind::TimedOut, "no reply"))
}

fn s07_udp() {
    let (server, server_addr) = udp();
    let (client, client_addr) = udp();
    // unconnected: address on every call. Message boundaries are preserved (unlike TCP).
    client.send_to(b"one", server_addr).unwrap();
    client.send_to(b"two-two", server_addr).unwrap();
    let mut buf = [0u8; 64];
    let (n, from) = server.peek_from(&mut buf).unwrap();
    assert_eq!((&buf[..n], from), (b"one".as_slice(), client_addr));
    for want in [b"one".as_slice(), b"two-two"] {
        let (n, from) = server.recv_from(&mut buf).unwrap();
        assert_eq!((&buf[..n], from), (want, client_addr)); // loopback in practice keeps order; UDP does not promise it
    }
    // truncation: the rest of the datagram is gone (unix behaviour; Windows returns an error)
    client.send_to(b"0123456789", server_addr).unwrap();
    client.send_to(b"next", server_addr).unwrap();
    #[cfg(unix)]
    {
        let mut small = [0u8; 4];
        let (n, _) = server.recv_from(&mut small).unwrap();
        assert_eq!(&small[..n], b"0123");
        let (n, _) = server.recv_from(&mut buf).unwrap();
        assert_eq!(
            &buf[..n],
            b"next",
            "no leftover bytes from the truncated datagram"
        );
    }
    #[cfg(not(unix))]
    {
        let _ = server.recv_from(&mut buf);
        let _ = server.recv_from(&mut buf);
    }

    // connect() on UDP sends nothing: it fixes the default peer and filters out datagrams from anyone else
    client.connect(server_addr).unwrap();
    assert_eq!(client.peer_addr().unwrap(), server_addr);
    let (stranger, _) = udp();
    stranger.send_to(b"spoof", client_addr).unwrap();
    // the server ignores the first two requests, to exercise retransmission
    let h = thread::spawn(move || {
        let mut buf = [0u8; 64];
        let mut seen = 0;
        loop {
            let (n, from) = server.recv_from(&mut buf).unwrap();
            seen += 1;
            if seen == 3 {
                server
                    .send_to(&buf[..n].to_ascii_uppercase(), from)
                    .unwrap();
                return seen;
            }
        }
    });
    let (reply, attempts) = udp_call(&client, b"hello", 50, Duration::from_millis(100)).unwrap();
    assert_eq!(reply, b"HELLO", "the stranger's datagram was filtered out");
    assert!(attempts >= 3);
    h.join().unwrap();

    let (lonely, _) = udp();
    let (silent, silent_addr) = udp();
    lonely.connect(silent_addr).unwrap();
    assert_eq!(
        udp_call(&lonely, b"?", 2, Duration::from_millis(10))
            .unwrap_err()
            .kind(),
        io::ErrorKind::TimedOut
    );
    drop(silent);

    // broadcast is off by default and must be enabled before sending to a broadcast address (nothing is sent here)
    assert!(!lonely.broadcast().unwrap());
    lonely.set_broadcast(true).unwrap();
    assert!(lonely.broadcast().unwrap());
    lonely.set_nonblocking(true).unwrap();
    println!("7. udp ok");
}

// ------------------------------------------------------------ 8. IPv6 loopback, when the host has it
fn s08_ipv6() {
    match TcpListener::bind("[::1]:0") {
        Ok(l) => {
            let addr = l.local_addr().unwrap();
            let mut c = connect(addr);
            let (mut s, peer) = l.accept().unwrap();
            assert!(peer.is_ipv6() && peer.ip().is_loopback());
            s.write_all(b"6").unwrap();
            let mut b = [0u8; 1];
            c.read_exact(&mut b).unwrap();
            println!("8. ipv6 loopback ok");
        }
        Err(e) => println!("8. ipv6 loopback unavailable here ({e}); skipped"),
    }
}

fn main() {
    s01_addresses();
    s02_cidr();
    s03_tcp_options();
    s04_echo();
    s05_framing();
    s06_shutdown();
    s07_udp();
    s08_ipv6();
    println!("all net sections passed");
}
