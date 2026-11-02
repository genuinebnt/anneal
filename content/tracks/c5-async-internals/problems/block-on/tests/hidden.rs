use solution::*;

fn within<T: Send + 'static>(ms: u64, f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(std::time::Duration::from_millis(ms)).ok()
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
        use std::time::{Duration, Instant};

        #[test]
        fn a_thread_wakes_it_and_it_does_not_spin() {
            let r = within(5000, || {
                let polls = std::cell::Cell::new(0);
                let t = Instant::now();
                block_on(async {
                    delay(30).await;
                    polls.set(polls.get() + 1);
                    delay(30).await;
                    polls.set(polls.get() + 1);
                    delay(30).await;
                    polls.set(polls.get() + 1);
                });
                (polls.get(), t.elapsed())
            });
            let (n, took) = r.expect("block_on hung: a wake from another thread was lost");
            check!("three sequential delays resumed three times", n, 3);
            assert!(took >= Duration::from_millis(85), "finished too early: {took:?}");
        }

        #[test]
        fn poll_count_stays_small_while_waiting() {
            let polls = within(5000, || {
                let polls = std::cell::Cell::new(0u32);
                let mut d = delay(120);
                block_on(std::future::poll_fn(|cx| {
                    polls.set(polls.get() + 1);
                    std::pin::Pin::new(&mut d).poll(cx)
                }));
                polls.get()
            })
            .expect("hung");
            assert!(polls <= 4, "polled {polls} times while waiting 120ms: that is spinning");
        }

        #[test]
        fn later_waits_do_not_spin_either() {
            let polls = within(5000, || {
                let polls = std::cell::Cell::new(0u32);
                let mut fut = Box::pin(async {
                    delay(40).await;
                    delay(40).await;
                    delay(40).await;
                });
                block_on(std::future::poll_fn(|cx| {
                    polls.set(polls.get() + 1);
                    fut.as_mut().poll(cx)
                }));
                polls.get()
            })
            .expect("hung");
            assert!(polls <= 8, "polled {polls} times across three 40ms waits: it stopped sleeping after the first wake");
        }

        #[test]
        fn many_independent_threads_wake_it_and_none_is_lost() {
            let ok = within(15_000, || {
                for _ in 0..300 {
                    // the wake can happen before block_on has parked; it must not be lost
                    let out = block_on(async {
                        let (tx, rx) = std::sync::mpsc::channel::<std::task::Waker>();
                        let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                        let d2 = done.clone();
                        let h = std::thread::spawn(move || {
                            let w = rx.recv().unwrap();
                            d2.store(true, std::sync::atomic::Ordering::SeqCst);
                            w.wake();
                        });
                        let mut sent = false;
                        std::future::poll_fn(|cx| {
                            if done.load(std::sync::atomic::Ordering::SeqCst) {
                                return std::task::Poll::Ready(());
                            }
                            if !sent {
                                sent = true;
                                tx.send(cx.waker().clone()).unwrap();
                            }
                            std::task::Poll::Pending
                        })
                        .await;
                        h.join().unwrap();
                        1
                    });
                    assert_eq!(out, 1);
                }
                true
            });
            assert_eq!(ok, Some(true), "a wake-up was lost and block_on slept forever");
        }

        #[test]
        fn waker_clone_can_outlive_the_poll() {
            let r = within(5000, || {
                let stash: std::sync::Arc<std::sync::Mutex<Option<std::task::Waker>>> = Default::default();
                let s2 = stash.clone();
                let h = std::thread::spawn(move || loop {
                    if let Some(w) = s2.lock().unwrap().take() {
                        std::thread::sleep(Duration::from_millis(40));
                        w.wake();
                        return;
                    }
                    std::thread::yield_now();
                });
                let mut first = true;
                block_on(std::future::poll_fn(move |cx| {
                    if first {
                        first = false;
                        *stash.lock().unwrap() = Some(cx.waker().clone());
                        std::task::Poll::Pending
                    } else {
                        std::task::Poll::Ready("woken")
                    }
                }))
                .to_string()
                    + &{ h.join().unwrap(); String::new() }
            });
            assert_eq!(r.as_deref(), Some("woken"));
        }

        #[test]
        fn nested_block_on_is_fine() {
            let r = within(5000, || block_on(async { block_on(async { 20 }) + block_on(async { delay(10).await; 22 }) }));
            assert_eq!(r, Some(42));
        }

        #[test]
        fn a_deep_chain_of_awaits() {
            fn count(n: u32) -> std::pin::Pin<Box<dyn std::future::Future<Output = u32>>> {
                Box::pin(async move { if n == 0 { 0 } else { 1 + count(n - 1).await } })
            }
            check!("recursion through boxed futures", block_on(count(500)), 500);
        }

        #[test]
        fn any_output_type_comes_back() {
            check!("a tuple of owned values", block_on(async { (String::from("a"), vec![1, 2], Some(3.5f64)) }), (String::from("a"), vec![1, 2], Some(3.5f64)));
            check!("unit", block_on(async {}), ());
        }

        #[test]
        fn a_future_that_returns_ready_immediately_is_polled_once() {
            let mut polls = 0;
            let v = block_on(std::future::poll_fn(|_| {
                polls += 1;
                std::task::Poll::Ready(9)
            }));
            check!("ready on the first poll", (v, polls), (9, 1));
        }
