use solution::*;

fn within<T: Send + 'static>(ms: u64, f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(std::time::Duration::from_millis(ms)).ok()
}

        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::{Arc, Barrier};
        use std::time::Duration;

        fn wait_for_pending(l: &RwLock<u32>, n: usize) {
            for _ in 0..400 {
                if l.pending_writers() == n {
                    return;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            panic!("never saw {n} pending writers");
        }

        #[test]
        fn a_waiting_writer_blocks_new_readers() {
            let l = Arc::new(RwLock::new(0u32));
            let r = l.read();
            let l2 = l.clone();
            let w = std::thread::spawn(move || {
                *l2.write() += 1;
            });
            wait_for_pending(&l, 1);
            check!("writer waits behind a reader: a new try_read must fail", l.try_read().is_none(), true);
            drop(r);
            within(2000, move || w.join().unwrap()).expect("writer never got in");
            check!("after the writer: reads work and see its write", l.try_read().map(|g| *g), Some(1));
        }

        #[test]
        fn blocked_reader_is_released_after_the_writer() {
            let l = Arc::new(RwLock::new(0u32));
            let w = l.write();
            let l2 = l.clone();
            let reader = std::thread::spawn(move || *l2.read());
            std::thread::sleep(Duration::from_millis(60));
            drop(w);
            check!("blocked read returns after the write guard drops", within(2000, move || reader.join().unwrap()), Some(0));
        }

        #[test]
        fn readers_really_share() {
            // four readers wait for each other at a barrier while all inside: impossible if reads were exclusive
            let l = Arc::new(RwLock::new(7u32));
            let b = Arc::new(Barrier::new(4));
            let hs: Vec<_> = (0..4)
                .map(|_| {
                    let (l, b) = (l.clone(), b.clone());
                    std::thread::spawn(move || {
                        let g = l.read();
                        b.wait();
                        *g
                    })
                })
                .collect();
            let vals = within(3000, move || hs.into_iter().map(|h| h.join().unwrap()).collect::<Vec<_>>());
            check!("4 readers inside together", vals, Some(vec![7, 7, 7, 7]));
        }

        #[test]
        fn writers_are_exclusive_and_see_consistent_pairs() {
            // invariant: the two halves are equal whenever the lock is free; a writer breaks it while it holds the lock
            let l = Arc::new(RwLock::new((0u64, 0u64)));
            let torn = Arc::new(AtomicUsize::new(0));
            let mut hs = Vec::new();
            for _ in 0..2 {
                let l = l.clone();
                hs.push(std::thread::spawn(move || {
                    for _ in 0..20_000 {
                        let mut g = l.write();
                        g.0 += 1;
                        std::hint::spin_loop();
                        g.1 += 1;
                    }
                }));
            }
            for _ in 0..4 {
                let (l, torn) = (l.clone(), torn.clone());
                hs.push(std::thread::spawn(move || {
                    for _ in 0..20_000 {
                        let g = l.read();
                        if g.0 != g.1 {
                            torn.fetch_add(1, Ordering::SeqCst);
                        }
                    }
                }));
            }
            let done = within(15_000, move || {
                for h in hs {
                    h.join().unwrap();
                }
            });
            assert!(done.is_some(), "deadlock");
            let (a, b) = *l.read();
            check!("2 writers x 20000 and 4 readers: torn reads, final pair", (torn.load(Ordering::SeqCst), a, b), (0, 40_000, 40_000));
        }

        #[test]
        fn a_stream_of_readers_cannot_starve_a_writer() {
            let l = Arc::new(RwLock::new(0u32));
            let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let readers: Vec<_> = (0..4)
                .map(|_| {
                    let (l, stop) = (l.clone(), stop.clone());
                    std::thread::spawn(move || {
                        while !stop.load(Ordering::Relaxed) {
                            let _g = l.read();
                            std::thread::sleep(Duration::from_micros(300)); // readers overlap almost always
                        }
                    })
                })
                .collect();
            std::thread::sleep(Duration::from_millis(30));
            let l2 = l.clone();
            let got = within(3000, move || {
                *l2.write() += 1;
            });
            stop.store(true, Ordering::Relaxed);
            for h in readers {
                h.join().unwrap();
            }
            assert!(got.is_some(), "the writer starved behind the readers");
            check!("writer got through", *l.read(), 1);
        }

        #[test]
        fn pending_writers_counts_waiters_only() {
            let l = Arc::new(RwLock::new(0u32));
            check!("idle", l.pending_writers(), 0);
            let w = l.write();
            check!("a holder is not pending", l.pending_writers(), 0);
            let hs: Vec<_> = (0..3).map(|_| { let l = l.clone(); std::thread::spawn(move || { *l.write() += 1; }) }).collect();
            wait_for_pending(&l, 3);
            drop(w);
            within(3000, move || { for h in hs { h.join().unwrap(); } }).expect("writers never finished");
            check!("all done: pending, value", (l.pending_writers(), *l.read()), (0, 3));
        }

        #[test]
        fn try_write_fails_while_readers_exist() {
            let l = RwLock::new(1);
            let r1 = l.read();
            let r2 = l.read();
            let a = l.try_write().is_none();
            drop(r1);
            let b = l.try_write().is_none();
            drop(r2);
            check!("two readers, then one, then none", (a, b, l.try_write().is_some()), (true, true, true));
        }

        #[test]
        fn into_inner_and_drop() {
            let l = RwLock::new(String::from("v"));
            check!("into_inner", l.into_inner(), "v".to_string());
        }

        #[test]
        fn bounds_are_the_right_ones() {
            fn sync<T: Sync>() {}
            fn send<T: Send>() {}
            sync::<RwLock<Vec<u8>>>();
            send::<RwLock<Vec<u8>>>();
            send::<RwLock<std::cell::Cell<u8>>>(); // Cell is Send, so the lock may move between threads
        }
