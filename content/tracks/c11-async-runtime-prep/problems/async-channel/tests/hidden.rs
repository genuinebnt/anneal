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

#[allow(dead_code, unused_imports)]
use std::sync::Mutex;

/// A thread-parking `block_on` for the tests with real threads.
#[allow(dead_code)]
fn block_on<F: Future>(f: F) -> F::Output {
    struct Unpark(std::thread::Thread);
    impl Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(Unpark(std::thread::current())));
    let mut cx = Context::from_waker(&waker);
    let mut f = std::pin::pin!(f);
    loop {
        if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
        std::thread::park();
    }
}

#[allow(dead_code)]
fn within<T: Send + 'static>(seconds: u64, f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(std::time::Duration::from_secs(seconds))
        .expect("the test hung or panicked: a wake-up was probably lost")
}

#[test]
fn a_cancelled_waiter_does_not_swallow_the_free_place() {
    let (tx, mut rx) = channel(1);
    let _ = now(tx.send(0));
    let (ca, wa) = counter();
    let (cb, wb) = counter();
    let mut a = Box::pin(tx.send(1));
    let mut b = Box::pin(tx.send(2));
    assert!(poll(&mut a, &wa).is_pending());
    assert!(poll(&mut b, &wb).is_pending());
    check!("room appears", now(rx.recv()), Poll::Ready(Some(0)));
    assert!(woken(&ca) + woken(&cb) >= 1, "nobody was woken although there is room");
    drop(a); // cancelled after it may have been the one woken
    assert!(woken(&cb) >= 1, "there is room, one send is still waiting, and it was never woken");
    check!("the survivor sends", poll(&mut b, &wb), Poll::Ready(Ok(())));
    check!("its value", now(rx.recv()), Poll::Ready(Some(2)));
}

#[test]
fn cancelled_before_room_appears() {
    let (tx, mut rx) = channel(1);
    let _ = now(tx.send(0));
    let (_ca, wa) = counter();
    let (cb, wb) = counter();
    let mut a = Box::pin(tx.send(1));
    let mut b = Box::pin(tx.send(2));
    assert!(poll(&mut a, &wa).is_pending());
    assert!(poll(&mut b, &wb).is_pending());
    drop(a);
    check!("room appears", now(rx.recv()), Poll::Ready(Some(0)));
    assert!(woken(&cb) >= 1, "the only waiting send was not woken");
    check!("it sends", poll(&mut b, &wb), Poll::Ready(Ok(())));
}

#[test]
fn the_latest_waker_of_a_send_is_woken() {
    let (tx, mut rx) = channel(1);
    let _ = now(tx.send(0));
    let (_old, w_old) = counter();
    let (new, w_new) = counter();
    let mut send = Box::pin(tx.send(1));
    assert!(poll(&mut send, &w_old).is_pending());
    assert!(poll(&mut send, &w_new).is_pending());
    let _ = now(rx.recv());
    assert!(woken(&new) >= 1, "the waker of the latest poll was not woken");
}

#[test]
fn the_latest_waker_of_recv_is_woken() {
    let (tx, mut rx) = channel(1);
    let (_old, w_old) = counter();
    let (new, w_new) = counter();
    let mut recv = Box::pin(rx.recv());
    assert!(poll(&mut recv, &w_old).is_pending());
    assert!(poll(&mut recv, &w_new).is_pending());
    let _ = now(tx.send(1));
    assert!(woken(&new) >= 1, "the waker of the latest poll was not woken");
}

#[test]
fn dropping_the_last_sender_wakes_recv() {
    let (tx, mut rx) = channel::<u8>(1);
    let tx2 = tx.clone();
    let (count, waker) = counter();
    let mut recv = Box::pin(rx.recv());
    assert!(poll(&mut recv, &waker).is_pending());
    drop(tx);
    check!("one sender is left, still waiting", poll(&mut recv, &waker), Poll::Pending);
    let before = woken(&count);
    drop(tx2);
    assert!(woken(&count) > before, "the last sender went away without waking the receiver");
    check!("the end", poll(&mut recv, &waker), Poll::Ready(None));
}

#[test]
fn dropping_the_receiver_wakes_every_waiting_send() {
    let (tx, rx) = channel(1);
    let _ = now(tx.send(0));
    let wakers: Vec<_> = (0..4).map(|_| counter()).collect();
    let mut sends: Vec<_> = (1..=4).map(|v| Box::pin(tx.send(v))).collect();
    for (send, (_, w)) in sends.iter_mut().zip(&wakers) {
        assert!(poll(send, w).is_pending());
    }
    drop(rx);
    for (i, (send, (count, w))) in sends.iter_mut().zip(&wakers).enumerate() {
        assert!(woken(count) >= 1, "send {i} was not woken when the receiver went away");
        check!(format!("send {i} gets its value back"), poll(send, w), Poll::Ready(Err(i + 1)));
    }
}

#[test]
fn queued_values_are_dropped_with_the_receiver() {
    let value = Arc::new(());
    let (tx, rx) = channel(3);
    for _ in 0..3 {
        let _ = now(tx.send(value.clone()));
    }
    check!("three in the queue", Arc::strong_count(&value), 4);
    drop(rx);
    check!("released although a sender is alive", Arc::strong_count(&value), 1);
    drop(tx);
}

#[test]
fn a_cancelled_recv_loses_nothing() {
    let (tx, mut rx) = channel(2);
    let (_c, waker) = counter();
    {
        let mut recv = Box::pin(rx.recv());
        assert!(poll(&mut recv, &waker).is_pending());
        let _ = now(tx.send(9));
        // woken, but cancelled before it is polled again
    }
    check!("the value is still there", now(rx.recv()), Poll::Ready(Some(9)));
}

#[test]
fn repeated_polls_do_not_pile_up_wakers() {
    let (tx, mut rx) = channel(1);
    let (count, waker) = counter();
    {
        let mut recv = Box::pin(rx.recv());
        for _ in 0..1000 {
            assert!(poll(&mut recv, &waker).is_pending());
        }
        assert!(Arc::strong_count(&count) <= 4, "recv keeps {} references to one waker", Arc::strong_count(&count));
    }
    let _ = now(tx.send(0));
    let (count, waker) = counter();
    let mut send = Box::pin(tx.send(1));
    for _ in 0..1000 {
        assert!(poll(&mut send, &waker).is_pending());
    }
    assert!(Arc::strong_count(&count) <= 4, "send keeps {} references to one waker", Arc::strong_count(&count));
}

#[test]
fn ends_and_futures_are_send() {
    fn is_send<T: Send>(_: &T) {}
    let (tx, mut rx) = channel::<std::cell::Cell<u8>>(1);
    is_send(&tx);
    is_send(&rx);
    {
        let (tx, mut rx) = channel::<String>(1);
        is_send(&tx.send(String::new()));
        is_send(&rx.recv());
    }
    let _ = rx.recv();
}

/// Random steps against a model. After every step the waking rule must hold.
#[test]
fn random_steps_keep_the_rules() {
    let mut rng = anneal_prelude::Rng::new(110_201);
    for round in 0..400 {
        let capacity = 1 + rng.below(3);
        let (tx, mut rx) = channel::<u32>(capacity);
        let mut model = std::collections::VecDeque::new();
        let mut waiting: Vec<(u32, Pin<Box<dyn Future<Output = Result<(), u32>> + '_>>, Arc<Count>, Waker, usize)> = Vec::new();
        let mut next = 0u32;
        let mut history = Vec::new();
        for _ in 0..40 {
            match rng.below(4) {
                0 => {
                    let v = next;
                    next += 1;
                    let (count, waker) = counter();
                    let mut send: Pin<Box<dyn Future<Output = Result<(), u32>> + '_>> = Box::pin(tx.send(v));
                    let had_room = model.len() < capacity;
                    let got = poll(&mut send, &waker);
                    history.push(format!("start send {v}"));
                    match got {
                        Poll::Ready(r) => {
                            check!(format!("round {round} {history:?}"), (r, had_room), (Ok(()), true));
                            model.push_back(v);
                        }
                        Poll::Pending => {
                            if waiting.is_empty() {
                                check!(format!("round {round} {history:?}: waited although there was room"), had_room, false);
                            }
                            waiting.push((v, send, count, waker, 0));
                        }
                    }
                }
                1 => {
                    let got = now(rx.recv());
                    history.push("recv".into());
                    match model.pop_front() {
                        Some(v) => check!(format!("round {round} {history:?}"), got, Poll::Ready(Some(v))),
                        None => check!(format!("round {round} {history:?}"), got, Poll::Pending),
                    }
                }
                2 if !waiting.is_empty() => {
                    let i = rng.below(waiting.len());
                    let gone = waiting.remove(i);
                    history.push(format!("cancel send {}", gone.0));
                }
                _ if !waiting.is_empty() => {
                    let i = rng.below(waiting.len());
                    let entry = &mut waiting[i];
                    entry.4 = woken(&entry.2); // wakes seen up to this poll
                    let (v, got) = (entry.0, poll(&mut entry.1, &entry.3));
                    history.push(format!("poll send {v}"));
                    if let Poll::Ready(r) = got {
                        check!(format!("round {round} {history:?}"), (r, model.len() < capacity), (Ok(()), true));
                        model.push_back(v);
                        waiting.remove(i);
                    }
                }
                _ => {}
            }
            check!(format!("round {round} {history:?}: queue length"), tx.len(), model.len());
            if model.len() < capacity && !waiting.is_empty() {
                let someone = waiting.iter().any(|w| woken(&w.2) > w.4);
                assert!(someone, "round {round} {history:?}: room is free, sends are waiting, none was woken");
            }
        }
    }
}

#[test]
fn threads_exchange_everything_in_order() {
    let ok = within(12, || {
        let (tx, mut rx) = channel::<(usize, u32)>(2);
        let senders: Vec<_> = (0..4)
            .map(|id| {
                let tx = tx.clone();
                std::thread::spawn(move || {
                    for i in 0..2000 {
                        block_on(tx.send((id, i))).unwrap();
                    }
                })
            })
            .collect();
        drop(tx);
        let mut next = [0u32; 4];
        while let Some((id, i)) = block_on(rx.recv()) {
            assert_eq!(i, next[id], "sender {id} out of order");
            next[id] += 1;
        }
        senders.into_iter().for_each(|s| s.join().unwrap());
        next
    });
    check!("all values from all senders", ok, [2000; 4]);
}
