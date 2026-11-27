use solution::*;

#[allow(dead_code, unused_imports)]
use std::future::Future;
#[allow(dead_code, unused_imports)]
use std::pin::Pin;
#[allow(dead_code, unused_imports)]
use std::sync::atomic::{AtomicUsize, Ordering};
#[allow(dead_code, unused_imports)]
use std::sync::Arc;
#[allow(dead_code, unused_imports)]
use std::task::{Context, Poll, Wake, Waker};

/// A waker that counts how often it was woken.
#[allow(dead_code)]
struct Count(AtomicUsize);
impl Wake for Count {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[allow(dead_code)]
fn counter() -> (Arc<Count>, Waker) {
    let count = Arc::new(Count(AtomicUsize::new(0)));
    let waker = Waker::from(count.clone());
    (count, waker)
}

#[allow(dead_code)]
fn woken(count: &Arc<Count>) -> usize {
    count.0.load(Ordering::SeqCst)
}

#[allow(dead_code)]
fn poll<F: Future + ?Sized>(f: &mut Pin<Box<F>>, waker: &Waker) -> Poll<F::Output> {
    f.as_mut().poll(&mut Context::from_waker(waker))
}

#[allow(dead_code)]
fn now<F: Future>(f: F) -> Poll<F::Output> {
    poll(&mut Box::pin(f), Waker::noop())
}

#[test]
fn a_value_goes_through() {
    let (tx, mut rx) = channel(2);
    check!("send with room", now(tx.send(1)), Poll::Ready(Ok(())));
    check!("receive it", now(rx.recv()), Poll::Ready(Some(1)));
}

#[test]
fn a_full_queue_makes_send_wait() {
    let (tx, mut rx) = channel(1);
    check!("fills the queue", now(tx.send('a')), Poll::Ready(Ok(())));
    let (count, waker) = counter();
    let mut second = Box::pin(tx.send('b'));
    check!("no room", poll(&mut second, &waker), Poll::Pending);
    check!("the queue holds only one", tx.len(), 1);
    check!("take the first", now(rx.recv()), Poll::Ready(Some('a')));
    assert!(woken(&count) >= 1, "the waiting send was not woken when room appeared");
    check!("now it fits", poll(&mut second, &waker), Poll::Ready(Ok(())));
    check!("and arrives", now(rx.recv()), Poll::Ready(Some('b')));
}

#[test]
fn an_empty_queue_makes_recv_wait() {
    let (tx, mut rx) = channel::<u8>(1);
    let (count, waker) = counter();
    let mut recv = Box::pin(rx.recv());
    check!("nothing yet", poll(&mut recv, &waker), Poll::Pending);
    check!("send", now(tx.send(7)), Poll::Ready(Ok(())));
    assert!(woken(&count) >= 1, "the receiver was not woken");
    check!("and gets it", poll(&mut recv, &waker), Poll::Ready(Some(7)));
}

#[test]
fn the_stream_ends_when_every_sender_is_gone() {
    let (tx, mut rx) = channel(4);
    let tx2 = tx.clone();
    let _ = now(tx.send(1));
    drop(tx);
    let _ = now(tx2.send(2));
    drop(tx2);
    check!("queued values first", now(rx.recv()), Poll::Ready(Some(1)));
    check!("second", now(rx.recv()), Poll::Ready(Some(2)));
    check!("then the end", now(rx.recv()), Poll::Ready(None));
    check!("again", now(rx.recv()), Poll::Ready(None));
}

#[test]
fn send_fails_when_the_receiver_is_gone() {
    let (tx, rx) = channel(1);
    drop(rx);
    check!("closed", tx.is_closed(), true);
    check!("the value comes back", now(tx.send(5)), Poll::Ready(Err(5)));
}

#[test]
fn dropping_a_waiting_send_queues_nothing() {
    let (tx, mut rx) = channel(1);
    let _ = now(tx.send(1));
    let (_c, waker) = counter();
    let mut second = Box::pin(tx.send(2));
    check!("waiting", poll(&mut second, &waker), Poll::Pending);
    drop(second);
    drop(tx);
    check!("only the first value", now(rx.recv()), Poll::Ready(Some(1)));
    check!("nothing else", now(rx.recv()), Poll::Ready(None));
}

#[test]
#[should_panic]
fn capacity_zero_panics() {
    let _ = channel::<u8>(0);
}
