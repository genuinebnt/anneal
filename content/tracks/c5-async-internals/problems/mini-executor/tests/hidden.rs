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

        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::{Arc, Mutex};
        use std::time::{Duration, Instant};

        #[test]
        fn run_sleeps_until_another_thread_wakes_a_task() {
            let r = within(5000, || {
                let ex = Executor::new();
                let h = ex.spawn(async { delay(60).await; "done" });
                let t = Instant::now();
                ex.run();
                (h.take(), t.elapsed())
            });
            let (out, took) = r.expect("run() hung or returned early");
            check!("the task finished with its output", out, Some("done"));
            assert!(took >= Duration::from_millis(55), "run returned after {took:?}, before the timer fired");
        }

        #[test]
        fn run_does_not_return_while_a_task_is_waiting() {
            let r = within(5000, || {
                let ex = Executor::new();
                let a = ex.spawn(async { delay(80).await; 1 });
                let b = ex.spawn(async { 2 });
                ex.run();
                (a.take(), b.take())
            });
            assert_eq!(r, Some((Some(1), Some(2))), "run() returned while task a was still waiting for its timer");
        }

        #[test]
        fn timers_run_concurrently_not_one_after_another() {
            let r = within(10_000, || {
                let ex = Executor::new();
                let done = Arc::new(AtomicUsize::new(0));
                for _ in 0..200 {
                    let done = done.clone();
                    ex.spawn(async move {
                        delay(100).await;
                        done.fetch_add(1, Ordering::SeqCst);
                    });
                }
                let t = Instant::now();
                ex.run();
                (done.load(Ordering::SeqCst), t.elapsed())
            });
            let (n, took) = r.expect("hung");
            check!("every task finished", n, 200);
            assert!(took < Duration::from_millis(2000), "200 x 100ms timers took {took:?}: tasks ran one at a time");
        }

        #[test]
        fn join_handle_wakes_a_waiting_parent() {
            let r = within(5000, || {
                let ex = Executor::new();
                let child = ex.spawn(async { delay(40).await; 6 });
                let parent = ex.spawn(async move { child.await * 7 });
                ex.run();
                parent.take()
            });
            assert_eq!(r, Some(Some(42)), "the parent was never woken when the child finished");
        }

        #[test]
        fn dropping_a_join_handle_does_not_cancel_the_task() {
            let ex = Executor::new();
            let flag = Arc::new(AtomicUsize::new(0));
            let f2 = flag.clone();
            drop(ex.spawn(async move {
                yield_now().await;
                f2.store(7, Ordering::SeqCst);
            }));
            ex.run();
            check!("the detached task still ran", flag.load(Ordering::SeqCst), 7);
        }

        #[test]
        fn fifo_order_for_plain_spawns() {
            let ex = Executor::new();
            let log = Arc::new(Mutex::new(Vec::new()));
            for i in 0..20 {
                let log = log.clone();
                ex.spawn(async move { log.lock().unwrap().push(i) });
            }
            ex.run();
            check!("spawn order is poll order", log.lock().unwrap().clone(), (0..20).collect::<Vec<_>>());
        }

        #[test]
        fn a_task_that_wakes_itself_while_being_polled() {
            let r = within(5000, || {
                let ex = Executor::new();
                let n = Arc::new(AtomicUsize::new(0));
                let n2 = n.clone();
                ex.spawn(async move {
                    for _ in 0..1000 {
                        n2.fetch_add(1, Ordering::SeqCst);
                        yield_now().await;
                    }
                });
                ex.run();
                n.load(Ordering::SeqCst)
            });
            assert_eq!(r, Some(1000));
        }

        #[test]
        fn many_tasks_with_random_yields() {
            let r = within(10_000, || {
                let mut rng = anneal_prelude::Rng::new(6701);
                let ex = Executor::new();
                let total = Arc::new(AtomicUsize::new(0));
                let mut want = 0;
                for _ in 0..2000 {
                    let yields = rng.below(6);
                    want += yields + 1;
                    let total = total.clone();
                    ex.spawn(async move {
                        for _ in 0..yields {
                            total.fetch_add(1, Ordering::SeqCst);
                            yield_now().await;
                        }
                        total.fetch_add(1, Ordering::SeqCst);
                    });
                }
                ex.run();
                (total.load(Ordering::SeqCst), want)
            });
            let (got, want) = r.expect("hung");
            check!("every step of every task ran exactly once", got, want);
        }

        #[test]
        fn a_deep_chain_of_joins() {
            let r = within(10_000, || {
                let ex = Executor::new();
                let mut prev = ex.spawn(async { 0u32 });
                for _ in 0..200 {
                    prev = ex.spawn(async move { yield_now().await; prev.await + 1 });
                }
                ex.run();
                prev.take()
            });
            assert_eq!(r, Some(Some(200)));
        }
