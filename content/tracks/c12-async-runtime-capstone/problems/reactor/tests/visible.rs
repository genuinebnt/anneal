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
fn bytes_cross_a_socket_pair() {
    within(8, || {
        let rt = Runtime::new(2);
        let reactor = Reactor::start(Poller::new());
        let (a, b) = pair(8);
        let (a, b) = (reactor.register(a), reactor.register(b));
        let server = rt.spawn(echo(b));
        let ok = rt.block_on(round_trip(a, b"hello, reactor, this is longer than the buffer".to_vec()));
        assert!(ok);
        assert_eq!(rt.block_on(server), Ok(46));
    });
}

#[test]
fn register_and_drop_are_counted() {
    let reactor = Reactor::start(Poller::new());
    let hand = Hand::default();
    let io = reactor.register(hand.clone());
    check!("one", reactor.registered(), 1);
    drop(io);
    check!("none", (reactor.registered(), hand.deregistered.load(Ordering::SeqCst)), (0, 1));
}

#[test]
fn an_event_wakes_a_waiting_read() {
    within(8, || {
        let reactor = Reactor::start(Poller::new());
        let hand = Hand::default();
        let io = reactor.register(hand.clone());
        let (_b, waker, rings) = bell();
        let mut cx = Context::from_waker(&waker);
        assert!(io.poll_read_with(&mut cx, |_| would_block::<()>()).is_pending());
        hand.fire(true, false);
        assert!(rang(&rings), "the event woke nobody");
    });
}

#[test]
fn read_exact_reports_an_early_end() {
    within(8, || {
        let rt = Runtime::new(1);
        let reactor = Reactor::start(Poller::new());
        let (a, b) = pair(16);
        let a = reactor.register(a);
        assert_eq!(b.try_write(b"abc").unwrap(), 3);
        drop(b);
        let got = rt.block_on(async { a.read_exact(&mut [0u8; 5]).await.map_err(|e| e.kind()) });
        assert_eq!(got, Err(ErrorKind::UnexpectedEof));
    });
}

#[test]
fn writing_to_a_gone_peer_fails() {
    within(8, || {
        let rt = Runtime::new(1);
        let reactor = Reactor::start(Poller::new());
        let (a, b) = pair(4);
        let a = reactor.register(a);
        drop(b);
        let got = rt.block_on(async { a.write_all(b"data").await.map_err(|e| e.kind()) });
        assert_eq!(got, Err(ErrorKind::BrokenPipe));
    });
}

#[test]
fn dropping_an_idle_reactor_returns() {
    within(8, || {
        let reactor = Reactor::start(Poller::new());
        std::thread::sleep(Duration::from_millis(20));
        drop(reactor);
    });
}
