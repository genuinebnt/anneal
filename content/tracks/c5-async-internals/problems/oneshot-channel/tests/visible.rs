use solution::*;

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
        use std::task::{Context, Poll, Wake, Waker};
        use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};

        struct Counter(AtomicUsize);
        impl Wake for Counter {
            fn wake(self: Arc<Self>) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }

        #[test]
        fn send_before_await() {
            let (tx, rx) = channel();
            tx.send(5).unwrap();
            check!("value waiting in the channel", block_on(rx), Ok(5));
        }

        #[test]
        fn send_after_pending_wakes_the_task() {
            let (tx, rx) = channel();
            let h = std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(40));
                tx.send("late").unwrap();
            });
            check!("receiver slept, then got the value", block_on(rx), Ok("late"));
            h.join().unwrap();
        }

        #[test]
        fn dropped_sender_closes_the_channel() {
            let (tx, rx) = channel::<u8>();
            drop(tx);
            check!("no value will ever come", block_on(rx), Err(RecvError));
        }

        #[test]
        fn send_to_a_dropped_receiver_returns_the_value() {
            let (tx, rx) = channel();
            drop(rx);
            check!("the value is handed back", (tx.is_closed(), tx.send(String::from("x"))), (true, Err(String::from("x"))));
        }

        #[test]
        fn pending_stores_the_waker_and_send_calls_it_once() {
            let c = Arc::new(Counter(AtomicUsize::new(0)));
            let w = Waker::from(c.clone());
            let mut cx = Context::from_waker(&w);
            let (tx, mut rx) = channel::<u32>();
            check!("nothing sent yet", Pin::new(&mut rx).poll(&mut cx), Poll::Pending);
            check!("not woken before the send", c.0.load(Ordering::SeqCst), 0);
            tx.send(7).unwrap();
            check!("woken exactly once by the send", c.0.load(Ordering::SeqCst), 1);
            check!("and now ready", Pin::new(&mut rx).poll(&mut cx), Poll::Ready(Ok(7)));
        }
