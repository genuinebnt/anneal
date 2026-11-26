//! RECIPE 430 (network): real UDP on loopback, socket options, then RELIABLE delivery built on a lossy datagram channel (stop-and-wait, Go-Back-N, Selective Repeat)
//! LEVEL: advanced
//! Use in: DNS/QUIC/game netcode/VoIP/discovery protocols, anything that needs TCP-like reliability without TCP's ordering or head-of-line blocking.   Run: cargo run --release --example udp_sockets_reliable
#![allow(dead_code)]
//!
//! WHAT: Part 1 uses REAL sockets on 127.0.0.1 to show what UDP does and does not promise: each `send` is ONE
//! datagram and each `recv` returns exactly one (message boundaries are preserved, unlike TCP's byte stream);
//! a receive buffer smaller than the datagram silently TRUNCATES it (the tail is lost, not queued); `connect()`
//! on a UDP socket only sets a default peer and a source filter (no handshake); a full receive queue DROPS
//! datagrams (SO_RCVBUF is the only back-pressure there is). It also tours the socket options that matter:
//! SO_REUSEADDR / SO_REUSEPORT, SO_RCVBUF / SO_SNDBUF, TCP_NODELAY, SO_KEEPALIVE and the TCP listen backlog.
//! Multicast needs an interface and routing that macOS loopback may not provide, so the demo tries it with TTL 0
//! (never leaves the host) and prints what happened instead of asserting.
//!
//! Part 2 adds what UDP lacks. Real loss is hard to reproduce on loopback, so we SIMULATE a datagram channel
//! (seeded xorshift: loss, duplication, reordering through extra random delay) in virtual 1 ms ticks, and
//! build three classic ARQ protocols on top. All three use sequence numbers and a retransmission timer whose
//! timeout comes from RFC 6298: SRTT/RTTVAR (Jacobson/Karels), RTO = SRTT + 4*RTTVAR, exponential backoff on
//! timeout, and KARN'S RULE (never take an RTT sample from a retransmitted packet: the ack is ambiguous).
//!  * STOP-AND-WAIT: one packet in flight; throughput = 1 packet per RTT, however fat the pipe.
//!  * GO-BACK-N: window of N, receiver accepts only in-order and sends CUMULATIVE acks ("next expected");
//!    on timeout the sender resends the whole window: simple receiver, wasteful retransmissions.
//!  * SELECTIVE REPEAT: receiver buffers out-of-order packets in its window and acks each (SELECTIVE ack plus a
//!    cumulative field); the sender keeps a timer per packet and resends only what is missing.
//! Each must deliver exactly 5000 messages in order with no loss, no duplicates, under 30% loss in BOTH
//! directions (acks are lost too), 5% duplication and a little reordering. The table compares efficiency.
//!
//! TCP does this in the kernel; QUIC and many game/VoIP stacks redo it in user space over UDP, usually with
//! the improvements here: selective acks, per-packet numbers that never repeat (no retransmission ambiguity)
//! and a window controlled by congestion control (see tcp_congestion_control_sim).
//!
//! COMMON USES: QUIC (RFC 9000), DNS, DHCP, NTP, SNMP, WireGuard, game state sync, RTP/VoIP, service discovery
//! (mDNS/SSDP use multicast), TFTP (literally stop-and-wait over UDP), reliable-UDP libraries such as ENet.
//! Read: RFC 768 (UDP) https://www.rfc-editor.org/rfc/rfc768 ; RFC 6298 (RTO computation)
//! https://www.rfc-editor.org/rfc/rfc6298 ; RFC 793 (TCP) https://www.rfc-editor.org/rfc/rfc793 ;
//! Kurose & Ross, "Computer Networking: A Top-Down Approach", ch. 3 (rdt, GBN, SR).
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::io::{self, ErrorKind, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream, UdpSocket};
use std::time::Duration;

const TMO: Duration = Duration::from_millis(500);

// ======================================================================================================
// PART 1: real UDP and socket options
// ======================================================================================================

fn udp_local() -> UdpSocket {
    let s = UdpSocket::bind("127.0.0.1:0").unwrap();
    s.set_read_timeout(Some(TMO)).unwrap();
    s.set_write_timeout(Some(TMO)).unwrap();
    s
}

fn is_timeout(e: &io::Error) -> bool {
    matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut)
}

fn demo_datagram_boundaries() {
    println!("-- datagram boundaries");
    let rx = udp_local();
    let tx = udp_local();
    let dst = rx.local_addr().unwrap();
    let msgs: [&[u8]; 4] = [b"a", b"bb", b"", b"dddd"];
    for m in msgs {
        tx.send_to(m, dst).unwrap();
    }
    let mut buf = [0u8; 64];
    for m in msgs {
        let (n, from) = rx.recv_from(&mut buf).unwrap();
        assert_eq!(
            &buf[..n],
            m,
            "one send = one recv, even for an EMPTY datagram"
        );
        assert_eq!(from, tx.local_addr().unwrap());
    }
    println!(
        "   4 sends (1, 2, 0, 4 bytes) -> 4 recvs with the same sizes; a TCP stream would give no such guarantee"
    );
}

fn demo_truncation() {
    println!("-- truncation");
    let rx = udp_local();
    let tx = udp_local();
    let dst = rx.local_addr().unwrap();
    tx.send_to(b"0123456789", dst).unwrap();
    tx.send_to(b"second", dst).unwrap();
    let mut small = [0u8; 4];
    match rx.recv_from(&mut small) {
        Ok((n, _)) => {
            assert_eq!((n, &small[..n]), (4, &b"0123"[..]));
            println!(
                "   10-byte datagram read into a 4-byte buffer -> got {n} bytes; the other 6 are GONE"
            );
        }
        // Windows reports WSAEMSGSIZE as an error (the buffer is still filled); Unix truncates silently.
        Err(e) => println!("   platform reports truncation as an error: {e}"),
    }
    let mut big = [0u8; 64];
    let (n, _) = rx.recv_from(&mut big).unwrap();
    assert_eq!(
        &big[..n],
        b"second",
        "the NEXT recv starts at the next datagram, not at byte 4"
    );
}

fn demo_connect() {
    println!("-- connect() on UDP");
    let a = udp_local();
    let b = udp_local();
    let stranger = udp_local();
    a.connect(b.local_addr().unwrap()).unwrap();
    b.connect(a.local_addr().unwrap()).unwrap();
    a.send(b"ping").unwrap(); // `send` works now: the default peer is set. No handshake happened.
    let mut buf = [0u8; 16];
    let n = b.recv(&mut buf).unwrap();
    assert_eq!(&buf[..n], b"ping");
    // A third party sends to the connected socket: the kernel filters it out (source != connected peer).
    stranger.send_to(b"spoof", a.local_addr().unwrap()).unwrap();
    b.send(b"pong").unwrap();
    let n = a.recv(&mut buf).unwrap();
    assert_eq!(&buf[..n], b"pong", "the stranger's datagram never shows up");
    a.set_read_timeout(Some(Duration::from_millis(100)))
        .unwrap();
    assert!(
        is_timeout(&a.recv(&mut buf).unwrap_err()),
        "nothing else is queued"
    );
    println!(
        "   connected socket ignored a datagram from another source; send/recv need no address"
    );
}

fn demo_broadcast_multicast() {
    println!("-- broadcast / multicast");
    let s = udp_local();
    println!("   SO_BROADCAST default = {}", s.broadcast().unwrap());
    s.set_broadcast(true).unwrap();
    assert!(s.broadcast().unwrap());
    println!(
        "   (broadcast = send to 255.255.255.255 or a subnet's .255; needs SO_BROADCAST; not sent here, it would leave the host)"
    );
    // Multicast: join a group on the any-interface, TTL 0 so packets never leave this machine, loopback on.
    let group = Ipv4Addr::new(239, 255, 42, 99);
    let rx = UdpSocket::bind("0.0.0.0:0").unwrap();
    rx.set_read_timeout(Some(Duration::from_millis(300)))
        .unwrap();
    let port = rx.local_addr().unwrap().port();
    let joined = rx.join_multicast_v4(&group, &Ipv4Addr::UNSPECIFIED);
    let tx = UdpSocket::bind("0.0.0.0:0").unwrap();
    tx.set_multicast_loop_v4(true).unwrap();
    tx.set_multicast_ttl_v4(0).unwrap();
    let sent = tx.send_to(b"hello group", SocketAddrV4::new(group, port));
    let mut buf = [0u8; 32];
    match (joined, sent, rx.recv_from(&mut buf)) {
        (Ok(()), Ok(_), Ok((n, _))) => {
            println!("   multicast join + loopback delivered {n} bytes to group {group}")
        }
        (j, s, r) => println!(
            "   multicast not delivered here (join={:?}, send={:?}, recv={:?}); it needs a multicast-capable interface route",
            j.map_err(|e| e.kind()),
            s.map_err(|e| e.kind()),
            r.map(|x| x.0).map_err(|e| e.kind())
        ),
    }
    println!(
        "   (receivers JOIN a group with IGMP/MLD; the sender just sends to 224.0.0.0/4 with a TTL controlling scope)"
    );
}

#[cfg(unix)]
mod sys {
    use std::io;
    use std::net::{Ipv4Addr, SocketAddrV4, TcpListener, UdpSocket};
    use std::os::fd::{AsRawFd, FromRawFd, RawFd};

    pub fn set_int(fd: RawFd, level: i32, name: i32, val: i32) -> io::Result<()> {
        // SAFETY: `val` outlives the call and the length passed is exactly size_of::<c_int>().
        let r = unsafe {
            libc::setsockopt(
                fd,
                level,
                name,
                &val as *const i32 as *const libc::c_void,
                size_of::<i32>() as libc::socklen_t,
            )
        };
        if r == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    pub fn get_int(fd: RawFd, level: i32, name: i32) -> io::Result<i32> {
        let mut val: i32 = 0;
        let mut len = size_of::<i32>() as libc::socklen_t;
        // SAFETY: `val` and `len` are valid, writable and correctly sized for an int-valued option.
        let r = unsafe {
            libc::getsockopt(
                fd,
                level,
                name,
                &mut val as *mut i32 as *mut libc::c_void,
                &mut len,
            )
        };
        if r == 0 {
            Ok(val)
        } else {
            Err(io::Error::last_os_error())
        }
    }

    fn sockaddr(addr: SocketAddrV4) -> libc::sockaddr_in {
        // SAFETY: sockaddr_in is plain old data; all-zero is a valid value (we then fill the used fields).
        let mut sa: libc::sockaddr_in = unsafe { std::mem::zeroed() };
        sa.sin_family = libc::AF_INET as libc::sa_family_t;
        sa.sin_port = addr.port().to_be();
        sa.sin_addr = libc::in_addr {
            s_addr: u32::from(*addr.ip()).to_be(),
        };
        sa
    }

    fn bind_fd(fd: RawFd, addr: SocketAddrV4) -> io::Result<()> {
        let sa = sockaddr(addr);
        // SAFETY: `sa` is a valid sockaddr_in and the length matches its type.
        let r = unsafe {
            libc::bind(
                fd,
                &sa as *const _ as *const libc::sockaddr,
                size_of::<libc::sockaddr_in>() as libc::socklen_t,
            )
        };
        if r == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    /// Create a UDP socket, apply SO_REUSEADDR / SO_REUSEPORT BEFORE bind (options only matter then), bind.
    pub fn udp_bind(
        addr: SocketAddrV4,
        reuse_addr: bool,
        reuse_port: bool,
    ) -> io::Result<UdpSocket> {
        // SAFETY: plain socket(2) call; the result is checked.
        let fd = unsafe { libc::socket(libc::AF_INET, libc::SOCK_DGRAM, 0) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `fd` is a freshly created descriptor that nothing else owns; UdpSocket now closes it on drop.
        let s = unsafe { UdpSocket::from_raw_fd(fd) };
        if reuse_addr {
            set_int(fd, libc::SOL_SOCKET, libc::SO_REUSEADDR, 1)?;
        }
        if reuse_port {
            set_int(fd, libc::SOL_SOCKET, libc::SO_REUSEPORT, 1)?;
        }
        bind_fd(fd, addr)?;
        Ok(s)
    }

    /// A TCP listener with an explicit listen(2) backlog (std always uses its own default).
    pub fn tcp_listen(backlog: i32) -> io::Result<TcpListener> {
        // SAFETY: plain socket(2) call; the result is checked.
        let fd = unsafe { libc::socket(libc::AF_INET, libc::SOCK_STREAM, 0) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: freshly created, exclusively owned descriptor; TcpListener closes it on drop.
        let l = unsafe { TcpListener::from_raw_fd(fd) };
        bind_fd(fd, SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))?;
        // SAFETY: valid bound stream socket.
        if unsafe { libc::listen(fd, backlog) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(l)
    }

    pub fn fd_of<T: AsRawFd>(t: &T) -> RawFd {
        t.as_raw_fd()
    }
}

#[cfg(unix)]
fn demo_socket_options() {
    use sys::*;
    println!("-- socket options (libc setsockopt/getsockopt)");
    // SO_REUSEPORT: several sockets bind the SAME port; the kernel load-balances datagrams across them.
    // This is how multi-threaded servers scale accept/recv without a shared lock.
    let first = udp_bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0), false, true).unwrap();
    let port = first.local_addr().unwrap().port();
    let addr = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
    let plain = udp_bind(addr, false, false);
    assert_eq!(
        plain.unwrap_err().kind(),
        ErrorKind::AddrInUse,
        "no option: second bind must fail"
    );
    let second =
        udp_bind(addr, false, true).expect("SO_REUSEPORT on both sockets allows sharing the port");
    let addr_only = udp_bind(addr, true, false);
    println!(
        "   bind same port: plain -> EADDRINUSE; both SO_REUSEPORT -> ok; SO_REUSEADDR only -> {} (semantics differ per OS)",
        match &addr_only {
            Ok(_) => "ok".to_string(),
            Err(e) => format!("{:?}", e.kind()),
        }
    );
    // Each datagram goes to one of the sharers; with many different senders both get some. (Distribution is
    // OS policy: macOS may favour one socket, Linux hashes the 4-tuple.) We only check nothing is lost.
    first
        .set_read_timeout(Some(Duration::from_millis(50)))
        .unwrap();
    second
        .set_read_timeout(Some(Duration::from_millis(50)))
        .unwrap();
    let mut seen = [0usize; 2];
    let mut senders = Vec::new();
    for i in 0..40u8 {
        let s = udp_local();
        s.send_to(&[i], addr).unwrap();
        senders.push(s);
    }
    let mut buf = [0u8; 8];
    for (i, sock) in [&first, &second].into_iter().enumerate() {
        while sock.recv_from(&mut buf).is_ok() {
            seen[i] += 1;
        }
    }
    println!(
        "   40 datagrams from 40 sources -> socket A got {}, socket B got {}",
        seen[0], seen[1]
    );
    assert_eq!(seen[0] + seen[1], 40);

    // SO_RCVBUF: a tiny receive queue overflows and the kernel silently drops (UDP has no back-pressure).
    let flood = |rcvbuf: Option<i32>| -> (i32, usize) {
        let rx = udp_local();
        if let Some(b) = rcvbuf {
            set_int(fd_of(&rx), libc::SOL_SOCKET, libc::SO_RCVBUF, b).unwrap();
        }
        let actual = get_int(fd_of(&rx), libc::SOL_SOCKET, libc::SO_RCVBUF).unwrap();
        let tx = udp_local();
        for i in 0..200u32 {
            let mut d = [0u8; 100];
            d[..4].copy_from_slice(&i.to_be_bytes());
            tx.send_to(&d, rx.local_addr().unwrap()).unwrap();
        }
        rx.set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        let mut n = 0;
        let mut buf = [0u8; 128];
        while rx.recv_from(&mut buf).is_ok() {
            n += 1;
        }
        (actual, n)
    };
    let (def_sz, def_n) = flood(None);
    let (tiny_sz, tiny_n) = flood(Some(1024));
    println!(
        "   flood 200 x 100 B datagrams unread: default SO_RCVBUF={def_sz} -> received {def_n}; requested 1024 (kernel gave {tiny_sz}) -> received {tiny_n}"
    );
    assert!(
        tiny_n < 200 && tiny_n < def_n.max(1) + 1,
        "a tiny receive buffer must lose datagrams"
    );
    let tx = udp_local();
    let snd = get_int(fd_of(&tx), libc::SOL_SOCKET, libc::SO_SNDBUF).unwrap();
    let big = tx.send_to(&vec![0u8; 20_000], "127.0.0.1:9");
    println!(
        "   SO_SNDBUF={snd}; sending one 20000-byte datagram -> {} (macOS caps UDP datagrams by net.inet.udp.maxdgram=9216; Linux allows up to 65507)",
        match big {
            Ok(n) => format!("ok, {n} bytes"),
            Err(e) => format!("error: {e}"),
        }
    );

    // TCP options on a real connection.
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let c = TcpStream::connect(l.local_addr().unwrap()).unwrap();
    let (srv, _) = l.accept().unwrap();
    c.set_nodelay(true).unwrap();
    assert!(c.nodelay().unwrap());
    drop(srv);
    set_int(fd_of(&c), libc::SOL_SOCKET, libc::SO_KEEPALIVE, 1).unwrap();
    assert_ne!(
        get_int(fd_of(&c), libc::SOL_SOCKET, libc::SO_KEEPALIVE).unwrap(),
        0
    );
    println!(
        "   TCP_NODELAY on (Nagle off: small writes go out immediately); SO_KEEPALIVE on (idle probes detect dead peers)"
    );

    // TCP backlog: listen(1) and never accept; extra connects beyond the queue stall (SYN dropped) or are refused.
    for backlog in [1, 128] {
        let l = tcp_listen(backlog).unwrap();
        let addr = l.local_addr().unwrap();
        let mut ok = 0;
        let mut conns = Vec::new();
        for _ in 0..12 {
            if let Ok(c) = TcpStream::connect_timeout(&addr, Duration::from_millis(100)) {
                ok += 1;
                conns.push(c);
            }
        }
        println!(
            "   listen backlog {backlog:3}: {ok}/12 connects completed without any accept() call (OSes round the backlog up; the rest are dropped SYNs)"
        );
        assert!(ok >= 1);
    }
}

#[cfg(not(unix))]
fn demo_socket_options() {
    println!(
        "-- socket options: libc demo skipped on this platform (std set_nodelay/set_broadcast still work)"
    );
}

// ======================================================================================================
// PART 2: reliable transport over a simulated lossy channel
// ======================================================================================================

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }
    fn chance(&mut self, p: f64) -> bool {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64 <= p
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// What travels on the wire. Data carries a payload; Ack carries a cumulative field (next expected) and an
/// optional selective field (the sequence number just received).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Pkt {
    Data { seq: u32, payload: u64 },
    Ack { cum: u32, sel: Option<u32> },
}

#[derive(Clone, Copy)]
struct ChanCfg {
    loss: f64,
    dup: f64,
    base_delay: u64, // ms one way
    reorder: f64, // chance a packet gets 1..=5 ms of extra delay: it then arrives AFTER its successors
}
// The link serialises one packet per ms (a 1000 packets/s bottleneck), so a burst is spread over time.

/// One direction of the simulated channel: a priority queue ordered by arrival time.
struct Channel {
    cfg: ChanCfg,
    heap: BinaryHeap<Reverse<(u64, u64, Pkt)>>,
    order: u64,
    next_free: u64,
    sent: u64,
    dropped: u64,
    duplicated: u64,
}

impl Channel {
    fn new(cfg: ChanCfg) -> Self {
        Channel {
            cfg,
            heap: BinaryHeap::new(),
            order: 0,
            next_free: 0,
            sent: 0,
            dropped: 0,
            duplicated: 0,
        }
    }
    fn send(&mut self, now: u64, p: Pkt, rng: &mut Rng) {
        self.sent += 1;
        if rng.chance(self.cfg.loss) {
            self.dropped += 1;
            return;
        }
        let copies = if rng.chance(self.cfg.dup) {
            self.duplicated += 1;
            2
        } else {
            1
        };
        for _ in 0..copies {
            let depart = now.max(self.next_free);
            self.next_free = depart + 1;
            let extra = if rng.chance(self.cfg.reorder) {
                1 + rng.below(5)
            } else {
                0
            };
            let at = depart + self.cfg.base_delay + extra;
            self.order += 1;
            self.heap.push(Reverse((at, self.order, p)));
        }
    }
    fn recv(&mut self, now: u64) -> Option<Pkt> {
        match self.heap.peek() {
            Some(Reverse((t, _, _))) if *t <= now => self.heap.pop().map(|Reverse((_, _, p))| p),
            _ => None,
        }
    }
}

/// RFC 6298 retransmission timer state (milliseconds, f64 for clarity).
#[derive(Clone, Debug)]
struct Rto {
    srtt: Option<f64>,
    rttvar: f64,
    rto: f64,
    backoff: u32, // exponent applied on top of rto
    min: f64,
    max: f64,
}

impl Rto {
    fn new(initial: f64, min: f64, max: f64) -> Self {
        Rto {
            srtt: None,
            rttvar: 0.0,
            rto: initial,
            backoff: 0,
            min,
            max,
        }
    }
    /// A VALID sample (never from a retransmitted packet, Karn). Also clears the exponential backoff.
    fn sample(&mut self, r: f64) {
        match self.srtt {
            None => {
                self.srtt = Some(r);
                self.rttvar = r / 2.0;
            }
            Some(s) => {
                // RTTVAR first (uses the OLD srtt), then SRTT: alpha = 1/8, beta = 1/4.
                self.rttvar = 0.75 * self.rttvar + 0.25 * (s - r).abs();
                self.srtt = Some(0.875 * s + 0.125 * r);
            }
        }
        self.rto = (self.srtt.unwrap() + (4.0 * self.rttvar).max(1.0)).clamp(self.min, self.max);
        self.backoff = 0;
    }
    /// Timeout: double (RFC 6298 rule 5.5), capped.
    fn on_timeout(&mut self) {
        self.backoff = (self.backoff + 1).min(10);
    }
    fn current(&self) -> u64 {
        (self.rto * (1u64 << self.backoff) as f64).min(self.max) as u64
    }
    /// Timer value for a packet that has already been retransmitted `n` times (per-packet backoff).
    fn for_attempt(&self, n: u32) -> u64 {
        (self.rto * (1u64 << n.min(10)) as f64).min(self.max) as u64
    }
}

fn payload_of(i: u32) -> u64 {
    let mut z = (i as u64)
        .wrapping_add(0x9E3779B97F4A7C15)
        .wrapping_mul(0xBF58476D1CE4E5B9);
    z ^= z >> 31;
    z.wrapping_mul(0x94D049BB133111EB)
}

#[derive(Default, Clone, Copy)]
struct TxStats {
    data_tx: u64,
    retx: u64,
}

trait Sender {
    fn on_ack(&mut self, now: u64, cum: u32, sel: Option<u32>);
    fn on_tick(&mut self, now: u64, out: &mut Vec<Pkt>);
    fn done(&self) -> bool;
    fn stats(&self) -> TxStats;
    fn rto(&self) -> &Rto;
}

trait Receiver {
    fn on_data(&mut self, seq: u32, payload: u64, delivered: &mut Vec<u64>) -> Option<Pkt>;
}

fn new_rto() -> Rto {
    Rto::new(200.0, 60.0, 2000.0)
}

// ---- stop-and-wait ----------------------------------------------------------------------------------
struct StopAndWait {
    n: u32,
    cur: u32,
    in_flight: bool,
    retx: bool,
    sent_at: u64,
    deadline: u64,
    rto: Rto,
    st: TxStats,
}

impl StopAndWait {
    fn new(n: u32) -> Self {
        StopAndWait {
            n,
            cur: 0,
            in_flight: false,
            retx: false,
            sent_at: 0,
            deadline: 0,
            rto: new_rto(),
            st: TxStats::default(),
        }
    }
}

impl Sender for StopAndWait {
    fn on_ack(&mut self, now: u64, cum: u32, _sel: Option<u32>) {
        if self.in_flight && cum > self.cur {
            if !self.retx {
                self.rto.sample((now - self.sent_at) as f64);
            }
            self.cur += 1;
            self.in_flight = false;
        }
    }
    fn on_tick(&mut self, now: u64, out: &mut Vec<Pkt>) {
        if self.cur >= self.n {
            return;
        }
        if !self.in_flight {
            out.push(Pkt::Data {
                seq: self.cur,
                payload: payload_of(self.cur),
            });
            self.st.data_tx += 1;
            self.in_flight = true;
            self.retx = false;
            self.sent_at = now;
            self.deadline = now + self.rto.current();
        } else if now >= self.deadline {
            self.rto.on_timeout();
            out.push(Pkt::Data {
                seq: self.cur,
                payload: payload_of(self.cur),
            });
            self.st.data_tx += 1;
            self.st.retx += 1;
            self.retx = true;
            self.deadline = now + self.rto.current();
        }
    }
    fn done(&self) -> bool {
        self.cur >= self.n
    }
    fn stats(&self) -> TxStats {
        self.st
    }
    fn rto(&self) -> &Rto {
        &self.rto
    }
}

// ---- go-back-n ----------------------------------------------------------------------------------------
struct GoBackN {
    n: u32,
    win: u32,
    base: u32,
    next: u32,
    sent_at: Vec<u64>,
    retx: Vec<bool>,
    deadline: Option<u64>,
    rto: Rto,
    st: TxStats,
}

impl GoBackN {
    fn new(n: u32, win: u32) -> Self {
        GoBackN {
            n,
            win,
            base: 0,
            next: 0,
            sent_at: vec![0; n as usize],
            retx: vec![false; n as usize],
            deadline: None,
            rto: new_rto(),
            st: TxStats::default(),
        }
    }
}

impl Sender for GoBackN {
    fn on_ack(&mut self, now: u64, cum: u32, _sel: Option<u32>) {
        if cum > self.base && cum <= self.next {
            let top = (cum - 1) as usize;
            if !self.retx[top] {
                self.rto.sample((now - self.sent_at[top]) as f64);
            }
            self.rto.backoff = 0; // forward progress: new data was acked, so clear the backoff (as Linux does)
            self.base = cum;
            self.deadline = if self.base < self.next {
                Some(now + self.rto.current())
            } else {
                None
            };
        }
    }
    fn on_tick(&mut self, now: u64, out: &mut Vec<Pkt>) {
        if let Some(d) = self.deadline {
            if now >= d {
                // Timeout: go back and resend EVERYTHING outstanding.
                self.rto.on_timeout();
                for s in self.base..self.next {
                    out.push(Pkt::Data {
                        seq: s,
                        payload: payload_of(s),
                    });
                    self.st.data_tx += 1;
                    self.st.retx += 1;
                    self.retx[s as usize] = true;
                    self.sent_at[s as usize] = now;
                }
                self.deadline = Some(now + self.rto.current());
            }
        }
        while self.next < self.n && self.next < self.base + self.win {
            let s = self.next;
            out.push(Pkt::Data {
                seq: s,
                payload: payload_of(s),
            });
            self.st.data_tx += 1;
            self.sent_at[s as usize] = now;
            if self.deadline.is_none() {
                self.deadline = Some(now + self.rto.current());
            }
            self.next += 1;
        }
    }
    fn done(&self) -> bool {
        self.base >= self.n
    }
    fn stats(&self) -> TxStats {
        self.st
    }
    fn rto(&self) -> &Rto {
        &self.rto
    }
}

// ---- selective repeat -----------------------------------------------------------------------------------
struct SelectiveRepeat {
    n: u32,
    win: u32,
    base: u32,
    next: u32,
    sent_at: Vec<u64>,
    attempts: Vec<u32>, // 0 = never retransmitted
    acked: Vec<bool>,
    deadline: Vec<u64>,
    rto: Rto,
    st: TxStats,
}

impl SelectiveRepeat {
    fn new(n: u32, win: u32) -> Self {
        let z = n as usize;
        SelectiveRepeat {
            n,
            win,
            base: 0,
            next: 0,
            sent_at: vec![0; z],
            attempts: vec![0; z],
            acked: vec![false; z],
            deadline: vec![0; z],
            rto: new_rto(),
            st: TxStats::default(),
        }
    }
    fn slide(&mut self) {
        while self.base < self.next && self.acked[self.base as usize] {
            self.base += 1;
        }
    }
}

impl Sender for SelectiveRepeat {
    fn on_ack(&mut self, now: u64, cum: u32, sel: Option<u32>) {
        if let Some(s) = sel {
            if s >= self.base && s < self.next && !self.acked[s as usize] {
                self.acked[s as usize] = true;
                if self.attempts[s as usize] == 0 {
                    self.rto.sample((now - self.sent_at[s as usize]) as f64);
                }
            }
        }
        // The cumulative field repairs lost selective acks: everything below `cum` is known delivered.
        for s in self.base..cum.min(self.next) {
            self.acked[s as usize] = true;
        }
        self.slide();
    }
    fn on_tick(&mut self, now: u64, out: &mut Vec<Pkt>) {
        for s in self.base..self.next {
            let i = s as usize;
            if !self.acked[i] && now >= self.deadline[i] {
                self.attempts[i] += 1;
                out.push(Pkt::Data {
                    seq: s,
                    payload: payload_of(s),
                });
                self.st.data_tx += 1;
                self.st.retx += 1;
                self.deadline[i] = now + self.rto.for_attempt(self.attempts[i]);
            }
        }
        while self.next < self.n && self.next < self.base + self.win {
            let i = self.next as usize;
            out.push(Pkt::Data {
                seq: self.next,
                payload: payload_of(self.next),
            });
            self.st.data_tx += 1;
            self.sent_at[i] = now;
            self.deadline[i] = now + self.rto.for_attempt(0);
            self.next += 1;
        }
    }
    fn done(&self) -> bool {
        self.base >= self.n
    }
    fn stats(&self) -> TxStats {
        self.st
    }
    fn rto(&self) -> &Rto {
        &self.rto
    }
}

// ---- receivers --------------------------------------------------------------------------------------------
/// Stop-and-wait and Go-Back-N receiver: accepts only the next expected packet, always acks "next expected".
struct InOrderReceiver {
    expected: u32,
}

impl Receiver for InOrderReceiver {
    fn on_data(&mut self, seq: u32, payload: u64, delivered: &mut Vec<u64>) -> Option<Pkt> {
        if seq == self.expected {
            delivered.push(payload);
            self.expected += 1;
        }
        Some(Pkt::Ack {
            cum: self.expected,
            sel: None,
        })
    }
}

/// Selective-repeat receiver: buffers out-of-order packets inside its window.
struct SrReceiver {
    base: u32,
    win: u32,
    buf: Vec<Option<u64>>,
}

impl Receiver for SrReceiver {
    fn on_data(&mut self, seq: u32, payload: u64, delivered: &mut Vec<u64>) -> Option<Pkt> {
        if seq >= self.base + self.win {
            return None; // beyond the window: a well-behaved sender never does this; do not buffer
        }
        if seq >= self.base {
            self.buf[seq as usize].get_or_insert(payload);
            while let Some(Some(p)) = self.buf.get(self.base as usize) {
                delivered.push(*p);
                self.base += 1;
            }
        }
        // Old duplicates (seq < base) are re-acked too, otherwise a lost ack would wedge the sender.
        Some(Pkt::Ack {
            cum: self.base,
            sel: Some(seq),
        })
    }
}

struct RunResult {
    name: &'static str,
    loss: f64,
    ticks: u64,
    data_tx: u64,
    retx: u64,
    acks: u64,
    fwd_dropped: u64,
    final_rto: u64,
    srtt: f64,
}

fn run(
    name: &'static str,
    n: u32,
    cfg: ChanCfg,
    seed: u64,
    mut tx: Box<dyn Sender>,
    mut rx: Box<dyn Receiver>,
) -> RunResult {
    let mut rng = Rng(seed);
    let mut fwd = Channel::new(cfg);
    let mut back = Channel::new(cfg);
    let mut delivered: Vec<u64> = Vec::with_capacity(n as usize);
    let mut now = 0u64;
    let mut out = Vec::new();
    while !tx.done() {
        now += 1;
        assert!(now < 20_000_000, "{name}: simulation did not converge");
        while let Some(p) = fwd.recv(now) {
            if let Pkt::Data { seq, payload } = p {
                if let Some(ack) = rx.on_data(seq, payload, &mut delivered) {
                    back.send(now, ack, &mut rng);
                }
            }
        }
        while let Some(p) = back.recv(now) {
            if let Pkt::Ack { cum, sel } = p {
                tx.on_ack(now, cum, sel);
            }
        }
        out.clear();
        tx.on_tick(now, &mut out);
        for p in out.drain(..) {
            fwd.send(now, p, &mut rng);
        }
    }
    // The exact oracle: every message, once, in order.
    let want: Vec<u64> = (0..n).map(payload_of).collect();
    assert_eq!(delivered.len(), want.len(), "{name}: wrong delivered count");
    assert!(delivered == want, "{name}: payload mismatch or reordering");
    let s = tx.stats();
    RunResult {
        name,
        loss: cfg.loss,
        ticks: now,
        data_tx: s.data_tx,
        retx: s.retx,
        acks: back.sent,
        fwd_dropped: fwd.dropped,
        final_rto: tx.rto().rto as u64,
        srtt: tx.rto().srtt.unwrap_or(0.0),
    }
}

fn run_protocol(proto: &'static str, n: u32, cfg: ChanCfg, seed: u64) -> RunResult {
    const WIN: u32 = 16;
    match proto {
        "stop-and-wait" => run(
            proto,
            n,
            cfg,
            seed,
            Box::new(StopAndWait::new(n)),
            Box::new(InOrderReceiver { expected: 0 }),
        ),
        "go-back-n" => run(
            proto,
            n,
            cfg,
            seed,
            Box::new(GoBackN::new(n, WIN)),
            Box::new(InOrderReceiver { expected: 0 }),
        ),
        _ => run(
            proto,
            n,
            cfg,
            seed,
            Box::new(SelectiveRepeat::new(n, WIN)),
            Box::new(SrReceiver {
                base: 0,
                win: WIN,
                buf: vec![None; n as usize],
            }),
        ),
    }
}

fn test_rto_estimator() {
    // RFC 6298 (2.2): first sample R -> SRTT=R, RTTVAR=R/2, RTO=SRTT+4*RTTVAR = 3R.
    let mut r = Rto::new(1000.0, 1.0, 60_000.0);
    r.sample(100.0);
    assert_eq!((r.srtt.unwrap(), r.rttvar, r.rto), (100.0, 50.0, 300.0));
    // Constant RTT: variance decays towards 0 and SRTT stays at R, so RTO converges to ~R.
    for _ in 0..200 {
        r.sample(100.0);
    }
    assert!(
        (r.srtt.unwrap() - 100.0).abs() < 1e-9 && r.rttvar < 1e-6 && (r.rto - 101.0).abs() < 0.01,
        "rto={}",
        r.rto
    );
    // Exponential backoff doubles per timeout; a fresh (valid) sample clears it.
    let base = r.current();
    r.on_timeout();
    r.on_timeout();
    r.on_timeout();
    assert_eq!(r.current(), base * 8);
    r.sample(100.0);
    assert_eq!(r.current(), base);
    // An RTT jump raises SRTT slowly (1/8) but RTTVAR immediately, so RTO reacts at once.
    let before = r.rto;
    r.sample(300.0);
    assert!(r.rto > before + 100.0 && r.srtt.unwrap() < 130.0);
    println!(
        "-- RTO estimator: first sample gives RTO = 3R; backoff x2 per timeout (x8 after 3); a variance spike lifts RTO at once"
    );
}

fn demo_reliable() {
    println!(
        "-- reliable transport over a simulated channel: 5000 messages, window 16, delay 20 ms each way, 1000 pkt/s link, 5% duplication and reordering"
    );
    test_rto_estimator();
    const N: u32 = 5000;
    let protos = ["stop-and-wait", "go-back-n", "selective-repeat"];
    let mut at30: Vec<RunResult> = Vec::new();
    println!(
        "   {:<17}{:>6}{:>11}{:>9}{:>8}{:>11}{:>11}{:>10}",
        "protocol", "loss", "tx/msg", "retx", "acks", "virt sec", "msgs/sec", "RTO ms"
    );
    for (li, loss) in [0.0, 0.1, 0.3].into_iter().enumerate() {
        let cfg = ChanCfg {
            loss,
            dup: 0.05,
            base_delay: 20,
            reorder: 0.05,
        };
        for (pi, p) in protos.iter().enumerate() {
            let r = run_protocol(p, N, cfg, 0xC0FFEE + (li * 10 + pi) as u64);
            let secs = r.ticks as f64 / 1000.0;
            println!(
                "   {:<17}{:>5.0}%{:>11.3}{:>9}{:>8}{:>11.1}{:>11.1}{:>10}",
                r.name,
                loss * 100.0,
                r.data_tx as f64 / N as f64,
                r.retx,
                r.acks,
                secs,
                N as f64 / secs,
                r.final_rto
            );
            if loss == 0.3 {
                at30.push(r);
            }
        }
    }
    let (sw, gbn, sr) = (&at30[0], &at30[1], &at30[2]);
    // Efficiency at 30% loss: SR retransmits only what was lost; GBN resends the window; SW is just slow.
    let txm = |r: &RunResult| r.data_tx as f64 / N as f64;
    assert!(txm(sr) < txm(gbn), "SR sends fewer packets than GBN");
    assert!(
        txm(sr) < 2.0,
        "SR near the theoretical 1/(1-p) = 1.43 (+ spurious): got {}",
        txm(sr)
    );
    assert!(
        sr.ticks < gbn.ticks && gbn.ticks < sw.ticks,
        "throughput order SR > GBN > stop-and-wait"
    );
    assert!(
        sw.ticks > 5 * sr.ticks,
        "windowing buys a large speed-up over stop-and-wait"
    );
    println!(
        "   at 30% loss: SR {:.2} tx/msg vs GBN {:.2} vs SW {:.2} (theory: SR 1/(1-p) = {:.2} data packets/msg, SW 1/(1-p)^2 = 2.04 because the ack must survive too); SR is {:.1}x faster than SW",
        txm(sr),
        txm(gbn),
        txm(sw),
        1.0 / 0.7,
        sw.ticks as f64 / sr.ticks as f64
    );
    println!(
        "   SRTT converged near the true round trip (40..50 ms): sw {:.0}, gbn {:.0}, sr {:.0} ms",
        sw.srtt, gbn.srtt, sr.srtt
    );
    for r in [sw, gbn, sr] {
        assert!(
            r.srtt > 35.0 && r.srtt < 120.0,
            "{} srtt {}",
            r.name,
            r.srtt
        );
        assert!(r.fwd_dropped > 0);
    }
}

fn main() {
    // A TCP sanity line so the Read/Write imports are exercised on a real stream.
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut c = TcpStream::connect(l.local_addr().unwrap()).unwrap();
    let (mut s, _) = l.accept().unwrap();
    s.set_read_timeout(Some(TMO)).unwrap();
    c.write_all(b"ab").unwrap();
    c.write_all(b"cd").unwrap();
    std::thread::sleep(Duration::from_millis(20));
    let mut buf = [0u8; 16];
    let n = s.read(&mut buf).unwrap();
    println!(
        "-- contrast: two TCP writes of 2 bytes arrived as one read of {n} bytes (stream, no boundaries)"
    );
    assert!(n >= 2 && &buf[..2] == b"ab");

    demo_datagram_boundaries();
    demo_truncation();
    demo_connect();
    demo_broadcast_multicast();
    demo_socket_options();
    demo_reliable();
    let _ = SocketAddr::from(([127, 0, 0, 1], 0));
    println!("udp_sockets_reliable: all checks passed");
}
