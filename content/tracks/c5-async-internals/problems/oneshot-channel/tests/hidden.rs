use solution::*;

fn within<T: Send + 'static>(ms: u64, f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(std::time::Duration::from_millis(ms)).ok()
}

fn block_on<F: std::future::Future>(f: F) -> F::Output {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};
    struct W {
        t: std::thread::Thread,
        flag: AtomicBool,
    }
    impl Wake for W {
        fn wake(self: Arc<Self>) {
            self.flag.store(true, Ordering::Release);
            self.t.unpark();
        }
    }
    let w = Arc::new(W { t: std::thread::current(), flag: AtomicBool::new(false) });
    let waker = Waker::from(w.clone());
    let mut cx = Context::from_waker(&waker);
    let mut f = std::pin::pin!(f);
    loop {
        if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
        while !w.flag.swap(false, Ordering::Acquire) {
            std::thread::park();
        }
    }
}

struct Delay {
    shared: std::sync::Arc<std::sync::Mutex<(bool, Option<std::task::Waker>)>>,
    started: bool,
    ms: u64,
}

fn delay(ms: u64) -> Delay {
    Delay { shared: std::sync::Arc::new(std::sync::Mutex::new((false, None))), started: false, ms }
}

impl std::future::Future for Delay {
    type Output = ();
    fn poll(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<()> {
        let mut g = self.shared.lock().unwrap();
        if g.0 {
            return std::task::Poll::Ready(());
        }
        g.1 = Some(cx.waker().clone());
        drop(g);
        if !self.started {
            self.started = true;
            let sh = self.shared.clone();
            let ms = self.ms;
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(ms));
                let mut g = sh.lock().unwrap();
                g.0 = true;
                if let Some(w) = g.1.take() {
                    drop(g);
                    w.wake();
                }
            });
        }
        std::task::Poll::Pending
    }
}

        use std::future::Future;
        use std::pin::Pin;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        use std::task::{Context, Poll, Wake, Waker};

        struct Counter(AtomicUsize);
        impl Wake for Counter {
            fn wake(self: Arc<Self>) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }

        #[test]
        fn the_newest_waker_is_the_one_woken() {
            let a = Arc::new(Counter(AtomicUsize::new(0)));
            let b = Arc::new(Counter(AtomicUsize::new(0)));
            let (wa, wb) = (Waker::from(a.clone()), Waker::from(b.clone()));
            let (tx, mut rx) = channel::<u32>();
            let _ = Pin::new(&mut rx).poll(&mut Context::from_waker(&wa));
            let _ = Pin::new(&mut rx).poll(&mut Context::from_waker(&wb));
            tx.send(1).unwrap();
            check!("old waker not called, new waker called once", (a.0.load(Ordering::SeqCst), b.0.load(Ordering::SeqCst)), (0, 1));
        }

        #[test]
        fn dropping_the_sender_wakes_a_waiting_receiver() {
            let c = Arc::new(Counter(AtomicUsize::new(0)));
            let w = Waker::from(c.clone());
            let (tx, mut rx) = channel::<u32>();
            let _ = Pin::new(&mut rx).poll(&mut Context::from_waker(&w));
            drop(tx);
            check!("woken by the drop", c.0.load(Ordering::SeqCst), 1);
            check!("and it reports closed", Pin::new(&mut rx).poll(&mut Context::from_waker(&w)), Poll::Ready(Err(RecvError)));
        }

        #[test]
        fn a_waiting_receiver_learns_the_sender_vanished() {
            let r = within(5000, || {
                let (tx, rx) = channel::<u32>();
                let h = std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(40));
                    drop(tx);
                });
                let out = block_on(rx);
                h.join().unwrap();
                out
            });
            assert_eq!(r, Some(Err(RecvError)), "the receiver hung or got a wrong result");
        }

        #[test]
        fn try_recv_states() {
            let (tx, mut rx) = channel::<u32>();
            check!("empty while the sender lives", rx.try_recv(), Err(TryRecvError::Empty));
            tx.send(3).unwrap();
            check!("the value", rx.try_recv(), Ok(3));
            check!("then closed (the sender is consumed)", rx.try_recv(), Err(TryRecvError::Closed));
            let (tx2, mut rx2) = channel::<u32>();
            drop(tx2);
            check!("closed without a value", rx2.try_recv(), Err(TryRecvError::Closed));
        }

        #[test]
        fn value_sent_just_before_the_sender_drops_is_not_lost() {
            let (tx, rx) = channel();
            tx.send(String::from("kept")).unwrap(); // the sender is dropped at the end of send
            check!("the value wins over closed", block_on(rx), Ok(String::from("kept")));
        }

        #[test]
        fn many_channels_across_threads() {
            let r = within(20_000, || {
                let mut handles = Vec::new();
                for i in 0..200u32 {
                    let (tx, rx) = channel();
                    handles.push(std::thread::spawn(move || block_on(rx)));
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis((i % 5) as u64));
                        tx.send(i).unwrap();
                    });
                }
                handles.into_iter().map(|h| h.join().unwrap().unwrap()).sum::<u32>()
            });
            assert_eq!(r, Some((0..200u32).sum()), "a channel lost its value or hung");
        }

        #[test]
        fn sending_a_large_value_moves_it() {
            let (tx, rx) = channel();
            let big: Vec<u64> = (0..100_000).collect();
            let ptr = big.as_ptr();
            tx.send(big).unwrap();
            let got = block_on(rx).unwrap();
            check!("the same allocation arrived", (got.as_ptr() == ptr, got.len()), (true, 100_000));
        }

        #[test]
        fn dropping_the_receiver_after_pending_is_clean() {
            let c = Arc::new(Counter(AtomicUsize::new(0)));
            let w = Waker::from(c.clone());
            let (tx, mut rx) = channel::<u32>();
            let _ = Pin::new(&mut rx).poll(&mut Context::from_waker(&w));
            drop(rx);
            check!("the sender sees it", (tx.is_closed(), tx.send(1)), (true, Err(1)));
        }
