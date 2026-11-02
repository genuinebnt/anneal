use solution::*;

fn within<T: Send + 'static>(ms: u64, f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(std::time::Duration::from_millis(ms)).ok()
}

        use std::sync::Arc;
        use std::time::Duration;

        #[test]
        fn pop_timeout_times_out_after_about_that_long() {
            let q = BlockingQueue::<u8>::new(1);
            let t = std::time::Instant::now();
            let r = q.pop_timeout(Duration::from_millis(80));
            let took = t.elapsed();
            check!("empty queue, 80ms timeout: result", r, Err(PopError::Timeout));
            assert!(took >= Duration::from_millis(80), "returned early after {took:?}");
            assert!(took < Duration::from_millis(1500), "took far too long: {took:?}");
        }

        #[test]
        fn pop_timeout_reports_closed_and_returns_items() {
            let q = BlockingQueue::new(2);
            q.push(5).unwrap();
            q.close();
            check!("closed with one item: item first, then Closed", (q.pop_timeout(Duration::from_millis(10)), q.pop_timeout(Duration::from_millis(10))), (Ok(5), Err(PopError::Closed)));
        }

        #[test]
        fn pop_timeout_gets_an_item_that_arrives_in_time() {
            let q = Arc::new(BlockingQueue::new(2));
            let q2 = q.clone();
            let h = std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(60));
                q2.push(42).unwrap();
            });
            check!("item pushed after 60ms, timeout 3s", q.pop_timeout(Duration::from_secs(3)), Ok(42));
            h.join().unwrap();
        }

        #[test]
        fn blocked_push_continues_after_a_pop() {
            let q = Arc::new(BlockingQueue::new(1));
            q.push(1).unwrap();
            let q2 = q.clone();
            let pushed = std::thread::spawn(move || q2.push(2));
            std::thread::sleep(Duration::from_millis(60));
            check!("second push is still waiting: len", q.len(), 1);
            check!("pop frees the slot", q.pop(), Some(1));
            check!("the blocked push finishes", within(2000, move || pushed.join().unwrap()), Some(Ok(())));
            check!("and its value is next", q.pop(), Some(2));
        }

        #[test]
        fn blocked_pop_continues_after_a_push() {
            let q = Arc::new(BlockingQueue::new(1));
            let q2 = q.clone();
            let popper = std::thread::spawn(move || q2.pop());
            std::thread::sleep(Duration::from_millis(60));
            q.push(9).unwrap();
            check!("the blocked pop gets the value", within(2000, move || popper.join().unwrap()), Some(Some(9)));
        }

        #[test]
        fn close_wakes_blocked_poppers_and_pushers() {
            let q = Arc::new(BlockingQueue::new(1));
            let poppers: Vec<_> = (0..3).map(|_| { let q = q.clone(); std::thread::spawn(move || q.pop()) }).collect();
            std::thread::sleep(Duration::from_millis(60));
            q.close();
            let r = within(2000, move || poppers.into_iter().map(|h| h.join().unwrap()).collect::<Vec<Option<u8>>>());
            check!("3 blocked pops after close", r, Some(vec![None, None, None]));

            let q = Arc::new(BlockingQueue::new(1));
            q.push(1u8).unwrap();
            let pushers: Vec<_> = (0..3u8).map(|i| { let q = q.clone(); std::thread::spawn(move || q.push(10 + i)) }).collect();
            std::thread::sleep(Duration::from_millis(60));
            q.close();
            let mut back = within(2000, move || pushers.into_iter().map(|h| h.join().unwrap()).collect::<Vec<_>>()).expect("pushers never woke up");
            back.sort();
            check!("3 blocked pushes after close give their values back", back, vec![Err(10), Err(11), Err(12)]);
            check!("the item queued before close is still there", (q.pop(), q.pop()), (Some(1), None));
        }

        #[test]
        fn one_item_wakes_only_one_of_two_blocked_poppers() {
            // a pop that wakes up to an empty queue must go back to waiting, not return None
            let q = Arc::new(BlockingQueue::new(4));
            let (tx, rx) = std::sync::mpsc::channel();
            let hs: Vec<_> = (0..2)
                .map(|i| {
                    let (q, tx) = (q.clone(), tx.clone());
                    std::thread::spawn(move || {
                        let r = q.pop();
                        tx.send((i, r)).unwrap();
                    })
                })
                .collect();
            std::thread::sleep(Duration::from_millis(80));
            q.push(11).unwrap();
            let first = rx.recv_timeout(Duration::from_secs(2)).expect("nobody got the item");
            check!("one popper got the item", first.1, Some(11));
            std::thread::sleep(Duration::from_millis(120));
            check!("the other popper must still be waiting", rx.try_recv().is_err(), true);
            q.close();
            let second = rx.recv_timeout(Duration::from_secs(2)).expect("close did not wake the second popper");
            check!("close releases it with None", second.1, None);
            for h in hs {
                h.join().unwrap();
            }
        }

        #[test]
        fn timeout_is_a_total_not_per_wakeup() {
            // Items are pushed and stolen back again: the popper wakes up with nothing to take, and must not restart its timeout
            let mut conclusive = 0;
            for _ in 0..12 {
                let q = Arc::new(BlockingQueue::new(4));
                let q2 = q.clone();
                let popper = std::thread::spawn(move || {
                    let t = std::time::Instant::now();
                    let r = q2.pop_timeout(Duration::from_millis(300));
                    (r, t.elapsed())
                });
                for _ in 0..3 {
                    std::thread::sleep(Duration::from_millis(90));
                    if q.try_push(1).is_ok() {
                        let _ = q.try_pop();
                    }
                }
                let (r, took) = popper.join().unwrap();
                if r == Err(PopError::Timeout) {
                    conclusive += 1;
                    assert!(took < Duration::from_millis(400), "timeout restarted on wake-ups: took {took:?} instead of about 300ms");
                    break;
                }
            }
            assert!(conclusive > 0, "the popper kept winning the race for the item; no conclusive round");
        }

        #[test]
        fn zero_capacity_panics() {
            assert!(std::panic::catch_unwind(|| BlockingQueue::<u8>::new(0)).is_err());
        }

        #[test]
        fn four_producers_four_consumers_exactly_once() {
            let q = Arc::new(BlockingQueue::new(8));
            let producers: Vec<_> = (0..4u64)
                .map(|p| {
                    let q = q.clone();
                    std::thread::spawn(move || {
                        for i in 0..20_000u64 {
                            q.push(p * 100_000 + i).unwrap();
                        }
                    })
                })
                .collect();
            let consumers: Vec<_> = (0..4)
                .map(|_| {
                    let q = q.clone();
                    std::thread::spawn(move || {
                        let mut got = Vec::new();
                        while let Some(v) = q.pop() {
                            got.push(v);
                        }
                        got
                    })
                })
                .collect();
            for p in producers {
                p.join().unwrap();
            }
            q.close();
            let mut all: Vec<u64> = within(10_000, move || consumers.into_iter().flat_map(|c| c.join().unwrap()).collect::<Vec<_>>()).expect("consumers never finished");
            all.sort_unstable();
            let mut want: Vec<u64> = (0..4u64).flat_map(|p| (0..20_000u64).map(move |i| p * 100_000 + i)).collect();
            want.sort_unstable();
            check!("4 producers x 20000 into 4 consumers through capacity 8: items match", all == want, true);
        }

        #[test]
        fn capacity_one_ping_pong() {
            let q = Arc::new(BlockingQueue::new(1));
            let q2 = q.clone();
            let prod = std::thread::spawn(move || {
                for i in 0..30_000u32 {
                    q2.push(i).unwrap();
                }
                q2.close();
            });
            let mut expect = 0u32;
            let mut ok = true;
            while let Some(v) = q.pop() {
                ok &= v == expect;
                expect += 1;
            }
            prod.join().unwrap();
            check!("30000 items through a one-slot queue, in order", (ok, expect), (true, 30_000));
        }

        #[test]
        fn random_single_thread_vs_model() {
            let mut rng = anneal_prelude::Rng::new(5101);
            for _ in 0..300 {
                let cap = rng.below(5) + 1;
                let q = BlockingQueue::new(cap);
                let mut model = std::collections::VecDeque::new();
                let mut closed = false;
                let mut log = Vec::new();
                for _ in 0..rng.below(30) {
                    match rng.below(5) {
                        0 | 1 => {
                            let v = rng.below(100) as u32;
                            let want = if closed || model.len() == cap { Err(v) } else { model.push_back(v); Ok(()) };
                            log.push(format!("try_push {v}"));
                            check!(format!("cap {cap}: {}", log.join(", ")), q.try_push(v), want);
                        }
                        2 | 3 => {
                            log.push("try_pop".to_string());
                            check!(format!("cap {cap}: {}", log.join(", ")), q.try_pop(), model.pop_front());
                        }
                        _ => {
                            q.close();
                            closed = true;
                            log.push("close".to_string());
                        }
                    }
                    check!(format!("cap {cap}: {} then len", log.join(", ")), q.len(), model.len());
                }
            }
        }
