use solution::*;

fn within<T: Send + 'static>(ms: u64, f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(std::time::Duration::from_millis(ms)).ok()
}

        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        use std::time::Duration;

        #[test]
        fn cloned_sender_keeps_the_channel_connected() {
            let (tx, rx) = channel(2);
            let tx2 = tx.clone();
            drop(tx);
            tx2.send(1).unwrap();
            check!("one of two senders dropped: still connected", rx.try_recv(), Ok(1));
            check!("and empty, not disconnected", rx.try_recv(), Err(TryRecvError::Empty));
            drop(tx2);
            check!("both dropped", rx.try_recv(), Err(TryRecvError::Disconnected));
        }

        #[test]
        fn cloned_receiver_keeps_senders_working() {
            let (tx, rx) = channel(2);
            let rx2 = rx.clone();
            drop(rx);
            check!("one of two receivers dropped", tx.send(1), Ok(()));
            check!("the other one gets it", rx2.recv(), Ok(1));
            drop(rx2);
            check!("both dropped", tx.send(2), Err(SendError(2)));
        }

        #[test]
        fn blocked_recv_wakes_when_the_last_sender_drops() {
            let (tx, rx) = channel::<u8>(2);
            let tx2 = tx.clone();
            let h = std::thread::spawn(move || rx.recv());
            std::thread::sleep(Duration::from_millis(60));
            drop(tx);
            std::thread::sleep(Duration::from_millis(60));
            drop(tx2);
            check!("recv blocked across the first drop, woken by the second", within(2000, move || h.join().unwrap()), Some(Err(RecvError)));
        }

        #[test]
        fn blocked_send_wakes_when_the_last_receiver_drops() {
            let (tx, rx) = channel(1);
            tx.send(1u8).unwrap();
            let h = std::thread::spawn(move || tx.send(2));
            std::thread::sleep(Duration::from_millis(60));
            drop(rx);
            check!("send blocked on a full channel gets its value back", within(2000, move || h.join().unwrap()), Some(Err(SendError(2))));
        }

        #[test]
        fn backpressure_is_exactly_the_capacity() {
            let (tx, rx) = channel(3);
            let sent = Arc::new(AtomicUsize::new(0));
            let s2 = sent.clone();
            let h = std::thread::spawn(move || {
                for i in 0..100u32 {
                    tx.send(i).unwrap();
                    s2.fetch_add(1, Ordering::SeqCst);
                }
            });
            std::thread::sleep(Duration::from_millis(100));
            check!("nobody receives: the sender is stuck after capacity sends", sent.load(Ordering::SeqCst), 3);
            check!("one recv frees one slot", rx.recv(), Ok(0));
            std::thread::sleep(Duration::from_millis(100));
            check!("exactly one more send went through", sent.load(Ordering::SeqCst), 4);
            let rest: Vec<u32> = rx.collect();
            h.join().unwrap();
            check!("the rest arrives in order", rest, (1..100u32).collect::<Vec<_>>());
        }

        #[test]
        fn recv_timeout_variants() {
            let (tx, rx) = channel::<u8>(1);
            let t = std::time::Instant::now();
            check!("nothing arrives", rx.recv_timeout(Duration::from_millis(80)), Err(RecvTimeoutError::Timeout));
            assert!(t.elapsed() >= Duration::from_millis(80));
            let tx2 = tx.clone();
            let h = std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(50));
                tx2.send(9).unwrap();
            });
            check!("a value arrives in time", rx.recv_timeout(Duration::from_secs(3)), Ok(9));
            h.join().unwrap();
            drop(tx);
            check!("all senders gone", rx.recv_timeout(Duration::from_secs(3)), Err(RecvTimeoutError::Disconnected));
        }

        #[test]
        fn iterator_ends_on_disconnect() {
            let (tx, rx) = channel(2);
            let h = std::thread::spawn(move || {
                for i in 0..500u32 {
                    tx.send(i).unwrap();
                }
            });
            let got: Vec<u32> = rx.collect();
            h.join().unwrap();
            check!("500 values through capacity 2 then the loop ends", got, (0..500u32).collect::<Vec<_>>());
        }

        #[test]
        fn four_senders_three_receivers_exactly_once() {
            let (tx, rx) = channel(4);
            let senders: Vec<_> = (0..4u64)
                .map(|p| {
                    let tx = tx.clone();
                    std::thread::spawn(move || {
                        for i in 0..20_000u64 {
                            tx.send(p * 1_000_000 + i).unwrap();
                        }
                    })
                })
                .collect();
            drop(tx);
            let receivers: Vec<_> = (0..3).map(|_| { let rx = rx.clone(); std::thread::spawn(move || rx.collect::<Vec<u64>>()) }).collect();
            drop(rx);
            for s in senders {
                s.join().unwrap();
            }
            let mut all: Vec<u64> = within(10_000, move || receivers.into_iter().flat_map(|r| r.join().unwrap()).collect::<Vec<_>>()).expect("receivers never saw the disconnect");
            all.sort_unstable();
            let mut want: Vec<u64> = (0..4u64).flat_map(|p| (0..20_000u64).map(move |i| p * 1_000_000 + i)).collect();
            want.sort_unstable();
            check!("every value delivered exactly once", all == want, true);
        }

        #[test]
        fn queued_values_are_dropped_with_the_channel() {
            struct D(Arc<AtomicUsize>);
            impl Drop for D {
                fn drop(&mut self) {
                    self.0.fetch_add(1, Ordering::SeqCst);
                }
            }
            let n = Arc::new(AtomicUsize::new(0));
            let (tx, rx) = channel(4);
            for _ in 0..3 {
                tx.send(D(n.clone())).ok().unwrap();
            }
            drop(rx.recv());
            drop(tx);
            drop(rx);
            check!("three sent, one received and dropped, two left in the channel", n.load(Ordering::SeqCst), 3);
        }

        #[test]
        fn zero_capacity_panics() {
            assert!(std::panic::catch_unwind(|| channel::<u8>(0)).is_err());
        }
