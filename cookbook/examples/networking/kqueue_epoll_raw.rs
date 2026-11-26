//! RECIPE 234 (network): a raw readiness poller over kqueue (macOS/BSD) and epoll (Linux) behind one trait; level vs edge; 20-client echo server.
//! LEVEL: advanced
//! Use in: understanding mio/tokio internals, custom event loops, proxies, anything with thousands of sockets per thread.   Run: cargo run --release --example kqueue_epoll_raw
#![allow(dead_code)]
//!
//! WHAT: "readiness" I/O: you register file descriptors with the kernel, sleep in ONE blocking call, and the kernel returns the subset that
//! can now be read/written without blocking. All sockets are nonblocking (`set_nonblocking` = fcntl O_NONBLOCK) so a spurious wakeup
//! costs one `WouldBlock`, never a stalled loop. Compare `select`/`poll`, which rescan the whole fd list per call (O(n)); kqueue and epoll
//! keep the interest set inside the kernel and return only ready fds (O(ready)).
//!   Linux:  `epoll_create1`, `epoll_ctl(ADD/MOD/DEL)`, `epoll_wait`. A token travels in `epoll_event.u64`.
//!   macOS:  `kqueue`, `kevent` (the SAME call both submits changes and collects events). A token travels in `kevent.udata`.
//!           kqueue has one filter per direction (EVFILT_READ, EVFILT_WRITE), so a readable+writable fd arrives as two events.
//! `trait Poller` hides the difference. Only the implementation for the host OS is compiled; the Linux one is written against `libc`
//! and kept correct by `cfg`, the macOS one is what runs here.
//!
//! LEVEL vs EDGE: level-triggered ("is it ready NOW?") re-reports every wait until you drain the fd, forgiving but chatty.
//! Edge-triggered (EPOLLET / EV_CLEAR) reports a transition ONCE; if you read only part of the data you are not told again, so you must
//! read/accept/write until `WouldBlock`. This recipe asserts the counts: a pipe with unread bytes yields an event on every wait in level
//! mode, but exactly one in edge mode until more data arrives. The echo server uses level for the listener and edge for connections.
//!
//! mio (mio_event_loop) is exactly this trait made portable: `Poll`/`Registry`/`Token`/`Interest`, edge-triggered everywhere (it emulates it on
//! Windows with IOCP/wepoll). tokio's reactor wraps mio: each socket gets a ScheduledIo holding readiness bits and the wakers of tasks
//! blocked on it; the driver thread does `poll`, sets readiness, and wakes those tasks. `read().await` = try read, on WouldBlock park the waker.
//!
//! COMMON USES: the event loop under nginx, Redis, libuv (Node.js), Netty, tokio and async-std/smol (via polling); game servers; load balancers.
//! Read: https://man7.org/linux/man-pages/man7/epoll.7.html , https://man.freebsd.org/cgi/man.cgi?query=kqueue , https://docs.rs/mio
use std::collections::HashMap;
use std::io::{self, ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::os::fd::{AsRawFd, RawFd};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
compile_error!("this recipe implements the Poller for Linux (epoll) and macOS (kqueue) only");

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Interest {
    pub readable: bool,
    pub writable: bool,
    /// edge-triggered (EPOLLET / EV_CLEAR) instead of level-triggered
    pub edge: bool,
}
impl Interest {
    pub const READ: Interest = Interest {
        readable: true,
        writable: false,
        edge: false,
    };
    pub const READ_EDGE: Interest = Interest {
        readable: true,
        writable: false,
        edge: true,
    };
    pub const RW_EDGE: Interest = Interest {
        readable: true,
        writable: true,
        edge: true,
    };
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Event {
    pub token: usize,
    pub readable: bool,
    pub writable: bool,
    /// peer hung up / error / EOF flagged by the kernel (a read will tell which)
    pub closed: bool,
}

pub trait Poller: Sized {
    fn new() -> io::Result<Self>;
    fn add(&mut self, fd: RawFd, token: usize, interest: Interest) -> io::Result<()>;
    fn modify(&mut self, fd: RawFd, token: usize, interest: Interest) -> io::Result<()>;
    fn delete(&mut self, fd: RawFd) -> io::Result<()>;
    /// Clears `events`, blocks up to `timeout` (None = forever), returns the number of events. A signal interruption returns Ok(0).
    fn wait(&mut self, events: &mut Vec<Event>, timeout: Option<Duration>) -> io::Result<usize>;
}

fn cvt(rc: i32) -> io::Result<i32> {
    if rc < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(rc)
    }
}

pub fn set_nonblocking(fd: RawFd) -> io::Result<()> {
    // SAFETY: fcntl on an fd we were handed; F_GETFL/F_SETFL take/return plain ints and touch no memory.
    unsafe {
        let flags = cvt(libc::fcntl(fd, libc::F_GETFL))?;
        cvt(libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK))?;
    }
    Ok(())
}

// ------------------------------------------------------------------ macOS / kqueue
#[cfg(target_os = "macos")]
mod sys {
    use super::*;
    use std::ptr;

    pub struct Kqueue {
        kq: RawFd,
        buf: Vec<libc::kevent>,
    }

    impl Kqueue {
        fn change(&self, fd: RawFd, filter: i16, flags: u16, token: usize) -> io::Result<()> {
            // SAFETY: kevent is plain-old-data; all-zero is a valid value.
            let mut kev: libc::kevent = unsafe { std::mem::zeroed() };
            kev.ident = fd as usize;
            kev.filter = filter;
            kev.flags = flags;
            kev.udata = token as *mut libc::c_void;
            // SAFETY: `kev` is a valid one-element changelist; nevents = 0 so no event buffer or timeout is needed.
            cvt(unsafe { libc::kevent(self.kq, &kev, 1, ptr::null_mut(), 0, ptr::null()) })?;
            Ok(())
        }
        fn apply(&mut self, fd: RawFd, token: usize, i: Interest) -> io::Result<()> {
            for (filter, want) in [
                (libc::EVFILT_READ, i.readable),
                (libc::EVFILT_WRITE, i.writable),
            ] {
                if want {
                    let clear = if i.edge { libc::EV_CLEAR } else { 0 };
                    self.change(fd, filter, libc::EV_ADD | libc::EV_ENABLE | clear, token)?;
                } else {
                    match self.change(fd, filter, libc::EV_DELETE, token) {
                        Err(e) if e.raw_os_error() == Some(libc::ENOENT) => {} // was not registered
                        r => r?,
                    }
                }
            }
            Ok(())
        }
    }

    impl Poller for Kqueue {
        fn new() -> io::Result<Self> {
            // SAFETY: kqueue() takes no arguments; the returned fd is owned by this struct and closed in Drop.
            let kq = cvt(unsafe { libc::kqueue() })?;
            // SAFETY: plain fcntl on our own fd.
            cvt(unsafe { libc::fcntl(kq, libc::F_SETFD, libc::FD_CLOEXEC) })?;
            // SAFETY: zeroed kevents are valid placeholders that the kernel overwrites.
            let buf = (0..64).map(|_| unsafe { std::mem::zeroed() }).collect();
            Ok(Kqueue { kq, buf })
        }
        fn add(&mut self, fd: RawFd, token: usize, i: Interest) -> io::Result<()> {
            self.apply(fd, token, i)
        }
        fn modify(&mut self, fd: RawFd, token: usize, i: Interest) -> io::Result<()> {
            self.apply(fd, token, i)
        }
        fn delete(&mut self, fd: RawFd) -> io::Result<()> {
            self.apply(
                fd,
                0,
                Interest {
                    readable: false,
                    writable: false,
                    edge: false,
                },
            )
        }
        fn wait(
            &mut self,
            events: &mut Vec<Event>,
            timeout: Option<Duration>,
        ) -> io::Result<usize> {
            events.clear();
            let ts = timeout.map(|d| libc::timespec {
                tv_sec: d.as_secs() as _,
                tv_nsec: d.subsec_nanos() as _,
            });
            let ts_ptr = ts.as_ref().map_or(ptr::null(), |t| t as *const _);
            // SAFETY: no changelist; `buf` is a valid writable array of `len` kevents; ts_ptr is null or points at `ts`, alive for the call.
            let n = unsafe {
                libc::kevent(
                    self.kq,
                    ptr::null(),
                    0,
                    self.buf.as_mut_ptr(),
                    self.buf.len() as i32,
                    ts_ptr,
                )
            };
            if n < 0 {
                let e = io::Error::last_os_error();
                return if e.kind() == ErrorKind::Interrupted {
                    Ok(0)
                } else {
                    Err(e)
                };
            }
            for k in &self.buf[..n as usize] {
                events.push(Event {
                    token: k.udata as usize,
                    readable: k.filter == libc::EVFILT_READ,
                    writable: k.filter == libc::EVFILT_WRITE,
                    closed: k.flags & (libc::EV_EOF | libc::EV_ERROR) != 0,
                });
            }
            Ok(events.len())
        }
    }

    impl Drop for Kqueue {
        fn drop(&mut self) {
            // SAFETY: we own `kq` and close it exactly once.
            unsafe { libc::close(self.kq) };
        }
    }
    pub type Os = Kqueue;
}

// ------------------------------------------------------------------ Linux / epoll
#[cfg(target_os = "linux")]
mod sys {
    use super::*;

    pub struct Epoll {
        ep: RawFd,
        buf: Vec<libc::epoll_event>,
    }

    impl Epoll {
        fn ctl(&self, op: i32, fd: RawFd, token: usize, i: Interest) -> io::Result<()> {
            let mut bits = 0u32;
            if i.readable {
                bits |= (libc::EPOLLIN | libc::EPOLLRDHUP) as u32;
            }
            if i.writable {
                bits |= libc::EPOLLOUT as u32;
            }
            if i.edge {
                bits |= libc::EPOLLET as u32;
            }
            let mut ev = libc::epoll_event {
                events: bits,
                u64: token as u64,
            };
            // SAFETY: `ev` is a valid epoll_event for the duration of the call (DEL ignores it but needs non-null on old kernels).
            cvt(unsafe { libc::epoll_ctl(self.ep, op, fd, &mut ev) })?;
            Ok(())
        }
    }

    impl Poller for Epoll {
        fn new() -> io::Result<Self> {
            // SAFETY: epoll_create1 takes only flags; the fd is owned by this struct and closed in Drop.
            let ep = cvt(unsafe { libc::epoll_create1(libc::EPOLL_CLOEXEC) })?;
            Ok(Epoll {
                ep,
                buf: vec![libc::epoll_event { events: 0, u64: 0 }; 64],
            })
        }
        fn add(&mut self, fd: RawFd, token: usize, i: Interest) -> io::Result<()> {
            self.ctl(libc::EPOLL_CTL_ADD, fd, token, i)
        }
        fn modify(&mut self, fd: RawFd, token: usize, i: Interest) -> io::Result<()> {
            self.ctl(libc::EPOLL_CTL_MOD, fd, token, i)
        }
        fn delete(&mut self, fd: RawFd) -> io::Result<()> {
            self.ctl(libc::EPOLL_CTL_DEL, fd, 0, Interest::READ)
        }
        fn wait(
            &mut self,
            events: &mut Vec<Event>,
            timeout: Option<Duration>,
        ) -> io::Result<usize> {
            events.clear();
            // round UP to whole milliseconds so a 100us timeout does not become a busy 0
            let ms = timeout.map_or(-1, |d| {
                d.as_micros().div_ceil(1000).min(i32::MAX as u128) as i32
            });
            // SAFETY: `buf` is a valid writable array of `len` epoll_events; the kernel fills at most that many.
            let n = unsafe {
                libc::epoll_wait(self.ep, self.buf.as_mut_ptr(), self.buf.len() as i32, ms)
            };
            if n < 0 {
                let e = io::Error::last_os_error();
                return if e.kind() == ErrorKind::Interrupted {
                    Ok(0)
                } else {
                    Err(e)
                };
            }
            for e in &self.buf[..n as usize] {
                // epoll_event is `packed` on x86_64: copy fields out by value, never take references.
                let (bits, data) = (e.events, e.u64);
                let hup = bits & (libc::EPOLLHUP | libc::EPOLLRDHUP | libc::EPOLLERR) as u32 != 0;
                events.push(Event {
                    token: data as usize,
                    readable: bits & libc::EPOLLIN as u32 != 0 || hup, // let the read discover EOF/error
                    writable: bits & libc::EPOLLOUT as u32 != 0,
                    closed: hup,
                });
            }
            Ok(events.len())
        }
    }

    impl Drop for Epoll {
        fn drop(&mut self) {
            // SAFETY: we own `ep` and close it exactly once.
            unsafe { libc::close(self.ep) };
        }
    }
    pub type Os = Epoll;
}

use sys::Os;

// ------------------------------------------------------------------ helpers: a nonblocking pipe
struct Pipe {
    r: RawFd,
    w: RawFd,
}
impl Pipe {
    fn new() -> io::Result<Pipe> {
        let mut fds = [0i32; 2];
        // SAFETY: pipe writes exactly two ints into the array.
        cvt(unsafe { libc::pipe(fds.as_mut_ptr()) })?;
        set_nonblocking(fds[0])?;
        set_nonblocking(fds[1])?;
        Ok(Pipe {
            r: fds[0],
            w: fds[1],
        })
    }
    fn write(&self, data: &[u8]) {
        // SAFETY: `data` is a valid readable buffer of the given length; w is our open fd.
        let n = unsafe { libc::write(self.w, data.as_ptr().cast(), data.len()) };
        assert_eq!(n, data.len() as isize);
    }
    /// Reads up to `max` bytes; returns how many (0 if empty).
    fn read(&self, max: usize) -> usize {
        let mut b = vec![0u8; max];
        // SAFETY: `b` is a valid writable buffer of `max` bytes; r is our open fd.
        let n = unsafe { libc::read(self.r, b.as_mut_ptr().cast(), max) };
        if n < 0 { 0 } else { n as usize }
    }
}
impl Drop for Pipe {
    fn drop(&mut self) {
        // SAFETY: both fds are owned by this struct and closed once.
        unsafe {
            libc::close(self.r);
            libc::close(self.w);
        }
    }
}

fn count_events<P: Poller>(p: &mut P, evs: &mut Vec<Event>, token: usize, wait: Duration) -> usize {
    p.wait(evs, Some(wait)).unwrap();
    evs.iter()
        .filter(|e| e.token == token && e.readable)
        .count()
}

fn level_vs_edge<P: Poller>() {
    let mut evs = Vec::new();
    // LEVEL: unread data is re-reported on every wait.
    let (mut p, pipe) = (P::new().unwrap(), Pipe::new().unwrap());
    p.add(pipe.r, 7, Interest::READ).unwrap();
    assert_eq!(
        count_events(&mut p, &mut evs, 7, Duration::from_millis(30)),
        0,
        "nothing yet"
    );
    pipe.write(b"0123456789");
    let counts: Vec<usize> = (0..3)
        .map(|_| count_events(&mut p, &mut evs, 7, Duration::from_secs(1)))
        .collect();
    assert_eq!(
        counts,
        [1, 1, 1],
        "level: reported again and again while data remains"
    );
    assert_eq!(pipe.read(4), 4); // partial drain: still readable
    assert_eq!(count_events(&mut p, &mut evs, 7, Duration::from_secs(1)), 1);
    assert_eq!(pipe.read(100), 6); // fully drained
    assert_eq!(
        count_events(&mut p, &mut evs, 7, Duration::from_millis(30)),
        0,
        "level: quiet once drained"
    );
    // delete: no more events even with data
    pipe.write(b"x");
    p.delete(pipe.r).unwrap();
    assert_eq!(
        count_events(&mut p, &mut evs, 7, Duration::from_millis(30)),
        0
    );
    println!(
        "level-triggered: events per wait with unread data = {counts:?}, then 0 after drain/delete"
    );

    // EDGE: one report per arrival, even if you never read.
    let (mut p, pipe) = (P::new().unwrap(), Pipe::new().unwrap());
    p.add(pipe.r, 9, Interest::READ_EDGE).unwrap();
    pipe.write(b"0123456789");
    let first = count_events(&mut p, &mut evs, 9, Duration::from_secs(1));
    let again: Vec<usize> = (0..3)
        .map(|_| count_events(&mut p, &mut evs, 9, Duration::from_millis(30)))
        .collect();
    assert_eq!(
        (first, again.as_slice()),
        (1, [0, 0, 0].as_slice()),
        "edge: reported once, data still unread"
    );
    pipe.write(b"!"); // a NEW arrival is a new edge
    assert_eq!(count_events(&mut p, &mut evs, 9, Duration::from_secs(1)), 1);
    assert_eq!(pipe.read(100), 11);
    println!(
        "edge-triggered: first={first}, then {again:?} with the data unread; new write -> 1 more event"
    );
}

fn timeouts<P: Poller>() {
    let mut p = P::new().unwrap();
    let mut evs = Vec::new();
    let t = Instant::now();
    assert_eq!(
        p.wait(&mut evs, Some(Duration::from_millis(60))).unwrap(),
        0
    );
    let el = t.elapsed();
    assert!(
        el >= Duration::from_millis(50) && el < Duration::from_secs(2),
        "slept {el:?}"
    );
    let t = Instant::now();
    assert_eq!(p.wait(&mut evs, Some(Duration::ZERO)).unwrap(), 0);
    assert!(
        t.elapsed() < Duration::from_millis(500),
        "zero timeout must not block"
    );
    println!("timeouts: 60ms wait slept {el:?}; zero timeout returned immediately");
}

// ------------------------------------------------------------------ echo server driven by the Poller
const LISTENER: usize = 0;
const CLIENTS: usize = 20;
const MSG_LEN: usize = 3000;

struct Conn {
    stream: TcpStream,
    out: Vec<u8>,
    pos: usize,
    want_write: bool,
}

#[derive(Debug, Default)]
struct Stats {
    accepted: usize,
    closed: usize,
    bytes_echoed: usize,
    waits: usize,
    max_open: usize,
}

fn serve<P: Poller>(p: &mut P, listener: TcpListener, expect: usize, deadline: Instant) -> Stats {
    listener.set_nonblocking(true).unwrap();
    p.add(listener.as_raw_fd(), LISTENER, Interest::READ)
        .unwrap(); // level: a missed accept is re-reported
    let (mut conns, mut next, mut st, mut evs) = (
        HashMap::<usize, Conn>::new(),
        1usize,
        Stats::default(),
        Vec::new(),
    );
    while st.closed < expect {
        assert!(Instant::now() < deadline, "echo server timed out: {st:?}");
        p.wait(&mut evs, Some(Duration::from_millis(100))).unwrap();
        st.waits += 1;
        for ev in evs.clone() {
            if ev.token == LISTENER {
                loop {
                    match listener.accept() {
                        Ok((s, _)) => {
                            s.set_nonblocking(true).unwrap();
                            s.set_nodelay(true).unwrap();
                            p.add(s.as_raw_fd(), next, Interest::READ_EDGE).unwrap();
                            conns.insert(
                                next,
                                Conn {
                                    stream: s,
                                    out: Vec::new(),
                                    pos: 0,
                                    want_write: false,
                                },
                            );
                            next += 1;
                            st.accepted += 1;
                            st.max_open = st.max_open.max(conns.len());
                        }
                        Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                        Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                        Err(e) => panic!("accept: {e}"),
                    }
                }
                continue;
            }
            let Some(c) = conns.get_mut(&ev.token) else {
                continue;
            };
            let (mut eof, mut buf) = (false, [0u8; 4096]);
            // edge-triggered: drain the socket completely
            loop {
                match c.stream.read(&mut buf) {
                    Ok(0) => {
                        eof = true;
                        break;
                    }
                    Ok(n) => c.out.extend_from_slice(&buf[..n]),
                    Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                    Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                    Err(_) => {
                        eof = true;
                        break;
                    }
                }
            }
            // flush as much as the socket takes
            while c.pos < c.out.len() {
                match c.stream.write(&c.out[c.pos..]) {
                    Ok(n) => {
                        c.pos += n;
                        st.bytes_echoed += n;
                    }
                    Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                    Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                    Err(_) => {
                        eof = true;
                        c.pos = c.out.len();
                    }
                }
            }
            let pending = c.pos < c.out.len();
            if !pending {
                c.out.clear();
                c.pos = 0;
            }
            if eof && !pending {
                let c = conns.remove(&ev.token).unwrap();
                p.delete(c.stream.as_raw_fd()).unwrap(); // deregister before close
                st.closed += 1;
            } else if pending != c.want_write {
                c.want_write = pending;
                let i = if pending {
                    Interest::RW_EDGE
                } else {
                    Interest::READ_EDGE
                };
                p.modify(c.stream.as_raw_fd(), ev.token, i).unwrap();
            }
        }
    }
    st
}

fn echo_demo<P: Poller>() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    let handles: Vec<_> = (0..CLIENTS)
        .map(|id| {
            thread::spawn(move || {
                let mut s = TcpStream::connect_timeout(&addr, Duration::from_secs(5)).unwrap();
                s.set_read_timeout(Some(Duration::from_secs(8))).unwrap();
                s.set_write_timeout(Some(Duration::from_secs(8))).unwrap();
                let msg: Vec<u8> = (0..MSG_LEN).map(|i| (i * 7 + id) as u8).collect();
                for chunk in msg.chunks(750) {
                    s.write_all(chunk).unwrap();
                }
                let mut back = vec![0u8; msg.len()];
                s.read_exact(&mut back).unwrap();
                assert_eq!(back, msg, "client {id}: echo mismatch");
            })
        })
        .collect();
    let mut p = P::new().unwrap();
    let st = serve(
        &mut p,
        listener,
        CLIENTS,
        Instant::now() + Duration::from_secs(12),
    );
    for h in handles {
        h.join().unwrap();
    }
    assert_eq!((st.accepted, st.closed), (CLIENTS, CLIENTS));
    assert_eq!(st.bytes_echoed, CLIENTS * MSG_LEN);
    println!(
        "echo: {CLIENTS} clients, {} bytes echoed, max {} open at once, {} poller waits",
        st.bytes_echoed, st.max_open, st.waits
    );
}

fn main() {
    println!(
        "backend: {}",
        if cfg!(target_os = "linux") {
            "epoll"
        } else {
            "kqueue"
        }
    );
    level_vs_edge::<Os>();
    timeouts::<Os>();
    echo_demo::<Os>();
    println!("r234 ok");
}
