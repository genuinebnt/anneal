use solution::sim::{pair, Event, Poller, Socket, Source};
use solution::*;

#[allow(dead_code, unused_imports)]
use std::future::{poll_fn, Future};
#[allow(dead_code, unused_imports)]
use std::io::{self, ErrorKind};
#[allow(dead_code, unused_imports)]
use std::pin::Pin;
#[allow(dead_code, unused_imports)]
use std::sync::atomic::{AtomicUsize, Ordering};
#[allow(dead_code, unused_imports)]
use std::sync::mpsc::{channel, Receiver, Sender};
#[allow(dead_code, unused_imports)]
use std::sync::{Arc, Mutex};
#[allow(dead_code, unused_imports)]
use std::task::{Context, Poll, Wake, Waker};
#[allow(dead_code, unused_imports)]
use std::time::Duration;

/// Turns a hang into a failure.
#[allow(dead_code)]
fn within<T: Send + 'static>(seconds: u64, f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(Duration::from_secs(seconds))
        .expect("the test hung or panicked: a lost wake-up, or a thread that never stops")
}

/// A waker that sends a message each time it is woken, so a test can wait for it.
#[allow(dead_code)]
struct Bell(Mutex<Sender<()>>, AtomicUsize);
impl Wake for Bell {
    fn wake(self: Arc<Self>) {
        self.1.fetch_add(1, Ordering::SeqCst);
        let _ = self.0.lock().unwrap().send(());
    }
}

#[allow(dead_code)]
fn bell() -> (Arc<Bell>, Waker, Receiver<()>) {
    let (tx, rx) = channel();
    let bell = Arc::new(Bell(Mutex::new(tx), AtomicUsize::new(0)));
    (bell.clone(), Waker::from(bell), rx)
}

#[allow(dead_code)]
fn rang(rx: &Receiver<()>) -> bool {
    rx.recv_timeout(Duration::from_secs(5)).is_ok()
}

#[allow(dead_code)]
fn poll<F: Future + ?Sized>(f: &mut Pin<Box<F>>, waker: &Waker) -> Poll<F::Output> {
    f.as_mut().poll(&mut Context::from_waker(waker))
}

/// A source the test drives by hand: it records where it was registered.
#[allow(dead_code)]
#[derive(Clone, Default)]
struct Hand {
    seen: Arc<Mutex<Option<(Arc<Poller>, usize)>>>,
    deregistered: Arc<AtomicUsize>,
}
impl Source for Hand {
    fn register(&self, poller: &Arc<Poller>, token: usize) {
        *self.seen.lock().unwrap() = Some((poller.clone(), token));
    }
    fn deregister(&self) {
        self.deregistered.fetch_add(1, Ordering::SeqCst);
    }
}
#[allow(dead_code)]
impl Hand {
    fn token(&self) -> usize {
        self.seen.lock().unwrap().as_ref().expect("not registered").1
    }
    fn fire(&self, readable: bool, writable: bool) {
        let (poller, token) = self.seen.lock().unwrap().clone().expect("not registered");
        poller.post(Event { token, readable, writable });
    }
}

#[allow(dead_code)]
fn would_block<T>() -> io::Result<T> {
    Err(ErrorKind::WouldBlock.into())
}

/// Runs two futures on the same task until both are done.
#[allow(dead_code)]
async fn both<A: Future, B: Future>(a: A, b: B) -> (A::Output, B::Output) {
    let (mut a, mut b) = (Box::pin(a), Box::pin(b));
    let (mut ra, mut rb) = (None, None);
    poll_fn(|cx| {
        if ra.is_none() {
            if let Poll::Ready(v) = a.as_mut().poll(cx) {
                ra = Some(v);
            }
        }
        if rb.is_none() {
            if let Poll::Ready(v) = b.as_mut().poll(cx) {
                rb = Some(v);
            }
        }
        if ra.is_some() && rb.is_some() {
            Poll::Ready((ra.take().unwrap(), rb.take().unwrap()))
        } else {
            Poll::Pending
        }
    })
    .await
}

#[allow(dead_code)]
fn pattern(len: usize, seed: usize) -> Vec<u8> {
    (0..len).map(|i| ((i * 7 + seed) % 251) as u8).collect()
}

/// Reads until the end and writes everything back.
#[allow(dead_code)]
async fn echo(io: Io<Socket>) -> usize {
    let mut buf = [0u8; 100];
    let mut total = 0;
    loop {
        let n = io.read(&mut buf).await.unwrap();
        if n == 0 {
            break;
        }
        io.write_all(&buf[..n]).await.unwrap();
        total += n;
    }
    io.shutdown_write();
    total
}

/// Sends `data` and reads it back at the same time.
#[allow(dead_code)]
async fn round_trip(io: Io<Socket>, data: Vec<u8>) -> bool {
    let io = &io;
    let send = async {
        io.write_all(&data).await.unwrap();
        io.shutdown_write();
    };
    let receive = async {
        let mut back = vec![0u8; data.len()];
        io.read_exact(&mut back).await.unwrap();
        let after = io.read(&mut [0u8; 8]).await.unwrap();
        back == data && after == 0
    };
    both(send, receive).await.1
}

#[test]
fn tokens_are_never_reused() {
    let poller = Poller::new();
    let reactor = Reactor::start(poller.clone());
    let mut tokens = std::collections::HashSet::new();
    for _ in 0..200 {
        let hand = Hand::default();
        let io = reactor.register(hand.clone());
        let used = hand.seen.lock().unwrap().clone().unwrap();
        assert!(Arc::ptr_eq(&used.0, &poller), "registered with another poller");
        assert!(tokens.insert(used.1), "token {} used twice", used.1);
        drop(io);
    }
}

#[test]
fn the_directions_are_independent() {
    within(8, || {
        let reactor = Reactor::start(Poller::new());
        let hand = Hand::default();
        let io = reactor.register(hand.clone());
        let (_r, read_waker, reads) = bell();
        let (_w, write_waker, writes) = bell();
        assert!(io.poll_read_with(&mut Context::from_waker(&read_waker), |_| would_block::<()>()).is_pending());
        assert!(io.poll_write_with(&mut Context::from_waker(&write_waker), |_| would_block::<()>()).is_pending());
        hand.fire(true, false);
        assert!(rang(&reads), "the reader was not woken");
        hand.fire(false, true);
        assert!(rang(&writes), "the writer was not woken");
    });
}

#[test]
fn two_waiters_in_one_direction_are_both_woken() {
    within(8, || {
        let reactor = Reactor::start(Poller::new());
        let hand = Hand::default();
        let io = reactor.register(hand.clone());
        let bells: Vec<_> = (0..2).map(|_| bell()).collect();
        for (_, waker, _) in &bells {
            assert!(io.poll_read_with(&mut Context::from_waker(waker), |_| would_block::<()>()).is_pending());
        }
        hand.fire(true, false);
        for (i, (_, _, rings)) in bells.iter().enumerate() {
            assert!(rang(rings), "waiter {i} was forgotten");
        }
    });
}

#[test]
fn repeated_polls_do_not_pile_up_wakers() {
    let reactor = Reactor::start(Poller::new());
    let hand = Hand::default();
    let io = reactor.register(hand);
    let (count, waker, _rings) = bell();
    for _ in 0..1000 {
        assert!(io.poll_read_with(&mut Context::from_waker(&waker), |_| would_block::<()>()).is_pending());
    }
    assert!(Arc::strong_count(&count) <= 4, "{} references to one waker", Arc::strong_count(&count));
}

/// The event of interest is handled by the reactor while `op` is still running. A second source, fired afterwards, shows
/// when the reactor is past it.
#[test]
fn an_event_during_the_attempt_is_not_lost() {
    within(8, || {
        let reactor = Reactor::start(Poller::new());
        let (mine, marker) = (Hand::default(), Hand::default());
        let io = reactor.register(mine.clone());
        let marker_io = reactor.register(marker.clone());
        let (count, waker, _rings) = bell();
        let mut calls = 0;
        let got = io.poll_read_with(&mut Context::from_waker(&waker), |_| {
            calls += 1;
            if calls > 1 {
                return Ok("data");
            }
            let (_m, marker_waker, marked) = bell();
            assert!(marker_io.poll_read_with(&mut Context::from_waker(&marker_waker), |_| would_block::<()>()).is_pending());
            mine.fire(true, false);
            marker.fire(true, false);
            assert!(rang(&marked));
            would_block()
        });
        match got {
            Poll::Ready(v) => assert_eq!(v.unwrap(), "data"),
            Poll::Pending => assert!(count.1.load(Ordering::SeqCst) >= 1, "pending, and the event is already gone"),
        }
    });
}

#[test]
fn interrupted_is_retried_and_errors_pass_through() {
    let reactor = Reactor::start(Poller::new());
    let io = reactor.register(Hand::default());
    let mut cx = Context::from_waker(Waker::noop());
    let mut calls = 0;
    let got = io.poll_read_with(&mut cx, |_| {
        calls += 1;
        if calls < 3 {
            Err(ErrorKind::Interrupted.into())
        } else {
            Ok(calls)
        }
    });
    check!("retried", got.map(|r| r.map_err(|e| e.kind())), Poll::Ready(Ok(3)));
    let got = io.poll_write_with(&mut cx, |_| Err::<(), _>(io::Error::from(ErrorKind::PermissionDenied)));
    check!("passed on", got.map(|r| r.map_err(|e| e.kind())), Poll::Ready(Err(ErrorKind::PermissionDenied)));
}

#[test]
fn unknown_tokens_are_ignored() {
    within(8, || {
        let poller = Poller::new();
        let reactor = Reactor::start(poller.clone());
        poller.post(Event { token: 12_345, readable: true, writable: true });
        let hand = Hand::default();
        let io = reactor.register(hand.clone());
        let gone = Hand::default();
        drop(reactor.register(gone.clone()));
        gone.fire(true, true);
        let (_b, waker, rings) = bell();
        assert!(io.poll_read_with(&mut Context::from_waker(&waker), |_| would_block::<()>()).is_pending());
        hand.fire(true, false);
        assert!(rang(&rings), "the reactor stopped working after an unknown token");
    });
}

#[test]
fn a_big_echo_through_tiny_buffers_on_one_task() {
    within(12, || {
        let rt = Runtime::new(2);
        let reactor = Reactor::start(Poller::new());
        let (a, b) = pair(16);
        let (a, b) = (reactor.register(a), reactor.register(b));
        let server = rt.spawn(echo(b));
        let data = pattern(100_000, 3);
        let client = rt.spawn(round_trip(a, data));
        rt.block_on(async {
            assert_eq!(client.await, Ok(true));
            assert_eq!(server.await, Ok(100_000));
        });
        check!("both sources dropped", reactor.registered(), 0);
    });
}

#[test]
fn many_clients_at_once() {
    within(12, || {
        let rt = Runtime::new(4);
        let reactor = Reactor::start(Poller::new());
        let mut clients = Vec::new();
        for id in 0..100 {
            let (a, b) = pair(1 + id % 30);
            rt.spawn(echo(reactor.register(b)));
            clients.push(rt.spawn(round_trip(reactor.register(a), pattern(500 + id * 13, id))));
        }
        rt.block_on(async {
            for (id, c) in clients.into_iter().enumerate() {
                assert_eq!(c.await, Ok(true), "client {id}");
            }
        });
    });
}

#[test]
fn ping_pong_one_byte_at_a_time() {
    within(12, || {
        let rt = Runtime::new(4);
        let reactor = Reactor::start(Poller::new());
        let (a, b) = pair(1);
        let (a, b) = (reactor.register(a), reactor.register(b));
        let pong = rt.spawn(async move {
            let mut byte = [0u8];
            for _ in 0..5000 {
                b.read_exact(&mut byte).await.unwrap();
                b.write_all(&byte).await.unwrap();
            }
        });
        let ping = rt.spawn(async move {
            let mut byte = [0u8];
            for i in 0..5000u32 {
                a.write_all(&[i as u8]).await.unwrap();
                a.read_exact(&mut byte).await.unwrap();
                assert_eq!(byte[0], i as u8);
            }
        });
        rt.block_on(async {
            ping.await.unwrap();
            pong.await.unwrap();
        });
    });
}

#[test]
fn reading_and_writing_one_io_from_two_tasks() {
    within(12, || {
        let rt = Runtime::new(4);
        let reactor = Reactor::start(Poller::new());
        let (a, b) = pair(5);
        let a = Arc::new(reactor.register(a));
        let server = rt.spawn(echo(reactor.register(b)));
        let data = pattern(20_000, 1);
        let (writer, want) = (a.clone(), data.clone());
        let sending = rt.spawn(async move {
            writer.write_all(&data).await.unwrap();
            writer.shutdown_write();
        });
        let receiving = rt.spawn(async move {
            let mut back = vec![0u8; 20_000];
            a.read_exact(&mut back).await.unwrap();
            back == want
        });
        rt.block_on(async {
            sending.await.unwrap();
            assert_eq!(receiving.await, Ok(true));
            server.await.unwrap();
        });
    });
}

#[test]
fn waiting_ends_when_the_peer_leaves() {
    within(8, || {
        let reactor = Reactor::start(Poller::new());
        // a waiting read sees the shutdown
        let (a, b) = pair(4);
        let a = reactor.register(a);
        let (_x, waker, rings) = bell();
        let mut buf = [0u8; 4];
        {
            let mut read = Box::pin(a.read(&mut buf));
            assert!(poll(&mut read, &waker).is_pending());
            b.shutdown_write();
            assert!(rang(&rings));
            assert_eq!(poll(&mut read, &waker).map(|r| r.unwrap()), Poll::Ready(0));
        }
        // a writer waiting for room sees the peer dropped
        let (c, d) = pair(2);
        let c = reactor.register(c);
        let (_y, waker, rings) = bell();
        let mut write = Box::pin(c.write_all(b"123456"));
        assert!(poll(&mut write, &waker).is_pending());
        drop(d);
        assert!(rang(&rings));
        match poll(&mut write, &waker) {
            Poll::Ready(Err(e)) => assert_eq!(e.kind(), ErrorKind::BrokenPipe),
            other => panic!("expected a broken pipe, got ready = {}", other.is_ready()),
        }
    });
}

#[test]
fn a_cancelled_read_loses_nothing() {
    within(8, || {
        let rt = Runtime::new(1);
        let reactor = Reactor::start(Poller::new());
        let (a, b) = pair(8);
        let a = reactor.register(a);
        {
            let mut buf = [0u8; 8];
            let mut read = Box::pin(a.read(&mut buf));
            let (_x, waker, rings) = bell();
            assert!(poll(&mut read, &waker).is_pending());
            b.try_write(b"kept").unwrap();
            assert!(rang(&rings));
        }
        let mut buf = [0u8; 4];
        rt.block_on(async { a.read_exact(&mut buf).await }).unwrap();
        assert_eq!(&buf, b"kept");
        let empty = rt.block_on(async { a.read(&mut []).await.map_err(|e| e.kind()) });
        assert_eq!(empty, Ok(0));
    });
}

#[test]
fn dropping_the_reactor_releases_the_waiters() {
    within(8, || {
        let reactor = Reactor::start(Poller::new());
        let (a, b) = pair(4);
        let (c, d) = pair(4);
        let (a, c) = (reactor.register(a), reactor.register(c));
        let (_x, read_waker, reads) = bell();
        let (_y, write_waker, writes) = bell();
        let mut buf = [0u8; 4];
        let mut read = Box::pin(a.read(&mut buf));
        assert!(poll(&mut read, &read_waker).is_pending());
        let mut write = Box::pin(c.write_all(b"too much data"));
        assert!(poll(&mut write, &write_waker).is_pending());
        drop(reactor);
        assert!(rang(&reads), "the waiting read was not woken");
        assert!(rang(&writes), "the waiting write was not woken");
        match poll(&mut read, &read_waker) {
            Poll::Ready(Err(e)) => assert_eq!(e.kind(), ErrorKind::Other),
            other => panic!("read: expected an error, got ready = {}", other.is_ready()),
        }
        match poll(&mut write, &write_waker) {
            Poll::Ready(Err(e)) => assert_eq!(e.kind(), ErrorKind::Other),
            other => panic!("write: expected an error, got ready = {}", other.is_ready()),
        }
        drop((b, d));
    });
}

#[test]
fn after_the_reactor_only_waiting_fails() {
    within(8, || {
        let rt = Runtime::new(1);
        let reactor = Reactor::start(Poller::new());
        let (a, b) = pair(16);
        let a = reactor.register(a);
        drop(reactor);
        b.try_write(b"ready").unwrap();
        let mut buf = [0u8; 5];
        rt.block_on(async { a.read_exact(&mut buf).await }).unwrap();
        assert_eq!(&buf, b"ready");
        let got = rt.block_on(async { a.read(&mut [0u8; 1]).await.map_err(|e| e.kind()) });
        assert_eq!(got, Err(ErrorKind::Other));
    });
}

#[test]
fn the_reactor_thread_has_exited_after_drop() {
    for _ in 0..50 {
        let poller = Poller::new();
        let reactor = Reactor::start(poller.clone());
        drop(reactor);
        // the thread's reference to the poller is gone with the thread
        check!("references to the poller", Arc::strong_count(&poller), 1);
    }
}

#[test]
fn types_cross_threads() {
    fn shared<T: Send + Sync>() {}
    fn send<T: Send>(_: &T) {}
    shared::<Io<Socket>>();
    shared::<Reactor>();
    let reactor = Reactor::start(Poller::new());
    let (a, _b) = pair(1);
    let io = reactor.register(a);
    send(&io.write_all(b""));
    let mut buf = [0u8; 1];
    send(&io.read_exact(&mut buf));
}
