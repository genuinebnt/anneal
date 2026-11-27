use solution::*;

#[allow(dead_code, unused_imports)]
use std::future::{poll_fn, Future};
#[allow(dead_code, unused_imports)]
use std::pin::Pin;
#[allow(dead_code, unused_imports)]
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
#[allow(dead_code, unused_imports)]
use std::sync::mpsc::channel;
#[allow(dead_code, unused_imports)]
use std::sync::{Arc, Barrier, Mutex};
#[allow(dead_code, unused_imports)]
use std::task::{Context, Poll, Waker};
#[allow(dead_code, unused_imports)]
use std::time::{Duration, Instant};

#[allow(dead_code)]
fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

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

/// `n` tasks that each block their thread until all `n` are running.
#[allow(dead_code)]
fn meet_up(n: usize) -> Vec<impl Future<Output = ()> + Send + 'static> {
    let barrier = Arc::new(Barrier::new(n));
    (0..n)
        .map(|_| {
            let barrier = barrier.clone();
            async move {
                barrier.wait();
            }
        })
        .collect()
}

#[allow(dead_code)]
struct DropFlag(Arc<AtomicBool>);
impl Drop for DropFlag {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

#[allow(dead_code)]
fn flag() -> (Arc<AtomicBool>, DropFlag) {
    let seen = Arc::new(AtomicBool::new(false));
    (seen.clone(), DropFlag(seen))
}

/// Never finishes; leaves its waker in `keep`.
#[allow(dead_code)]
async fn parked(guard: DropFlag, keep: Arc<Mutex<Vec<Waker>>>) {
    let _guard = guard;
    poll_fn(|cx| {
        keep.lock().unwrap().push(cx.waker().clone());
        Poll::Pending
    })
    .await
}

#[allow(dead_code)]
fn poll_once<T>(handle: &mut JoinHandle<T>) -> Poll<Result<T, JoinError>> {
    Pin::new(handle).poll(&mut Context::from_waker(Waker::noop()))
}

#[test]
fn every_worker_count_can_run_that_many_at_once() {
    for n in 1..=8 {
        within(8, move || {
            let rt = Runtime::new(n);
            // spawned from inside one task, so they start out close together
            rt.block_on(async move {
                let parent = spawn(async move {
                    let handles: Vec<_> = meet_up(n).into_iter().map(spawn).collect();
                    handles
                });
                for h in parent.await.unwrap() {
                    h.await.unwrap();
                }
            });
        });
    }
}

#[test]
fn a_busy_spawner_does_not_hold_its_children() {
    within(8, || {
        let rt = Runtime::new(4);
        let parent = rt.spawn(async {
            let barrier = Arc::new(Barrier::new(4));
            for _ in 0..3 {
                let barrier = barrier.clone();
                spawn(async move {
                    barrier.wait();
                });
            }
            // this thread stays busy until all three children run elsewhere
            barrier.wait();
        });
        rt.block_on(parent).unwrap();
    });
}

#[test]
fn barriers_from_outside_many_times_over() {
    within(12, || {
        let rt = Runtime::new(4);
        for _ in 0..4000 {
            let handles: Vec<_> = meet_up(4).into_iter().map(|f| rt.spawn(f)).collect();
            rt.block_on(async {
                for h in handles {
                    h.await.unwrap();
                }
            });
        }
    });
}

/// Forces one schedule with `probe`. While worker X moves a batch out of the shared queue, worker Y looks for work, sees
/// none of the batch and goes to sleep. Then X runs a task that holds its thread until the rest of the batch has run.
#[test]
fn a_worker_that_slept_during_a_batch_move_is_woken() {
    within(12, || {
        let rt = Runtime::new(2);
        let (started_tx, started) = channel();
        let (release_x, x_released) = channel::<()>();
        let (release_y, y_released) = channel::<()>();
        let (asleep_tx, asleep) = channel();
        let (moved_tx, moved) = channel();
        // two tasks hold both workers while the batch is put into the shared queue
        let x_started = started_tx.clone();
        rt.spawn(async move {
            x_started.send(()).unwrap();
            x_released.recv().unwrap();
            probe::on_next_batch_move(move || {
                release_y.send(()).unwrap();
                moved_tx.send(asleep.recv_timeout(Duration::from_secs(5)).is_ok()).unwrap();
            });
        });
        rt.spawn(async move {
            started_tx.send(()).unwrap();
            y_released.recv().unwrap();
            probe::on_next_sleep(move || {
                let _ = asleep_tx.send(());
            });
        });
        started.recv().unwrap();
        started.recv().unwrap();
        const OTHERS: usize = 7;
        let (ran_tx, ran) = channel();
        let first = rt.spawn(async move {
            (0..OTHERS).all(|_| ran.recv_timeout(Duration::from_secs(5)).is_ok())
        });
        for _ in 0..OTHERS {
            let ran = ran_tx.clone();
            rt.spawn(async move {
                let _ = ran.send(());
            });
        }
        release_x.send(()).unwrap();
        let all_ran = rt.block_on(first);
        match moved.try_recv() {
            Err(_) => panic!("no batch move was seen: take from the shared queue with the given `take_injected`"),
            Ok(slept) => assert!(slept, "the idle worker did not go to sleep in the given `Gate`"),
        }
        assert_eq!(all_ran, Ok(true), "a worker stayed asleep while tasks of a moved batch were waiting");
    });
}

#[test]
fn work_arriving_while_everyone_sleeps_is_picked_up() {
    within(12, || {
        for n in [1, 3] {
            let rt = Runtime::new(n);
            for i in 0..4000 {
                let h = rt.spawn(async move { i });
                assert_eq!(rt.block_on(h), Ok(i));
                if i % 500 == 0 {
                    std::thread::sleep(ms(2));
                }
            }
        }
    });
}

#[test]
fn fifty_thousand_small_tasks() {
    let (sum, runs) = within(12, || {
        let rt = Runtime::new(4);
        let runs: Arc<Vec<AtomicUsize>> = Arc::new((0..50_000).map(|_| AtomicUsize::new(0)).collect());
        let handles: Vec<_> = (0..50_000)
            .map(|i| {
                let runs = runs.clone();
                rt.spawn(async move {
                    runs[i].fetch_add(1, Ordering::SeqCst);
                    if i % 5 == 0 {
                        yield_now().await;
                    }
                    i
                })
            })
            .collect();
        let sum = rt.block_on(async {
            let mut sum = 0;
            for h in handles {
                sum += h.await.unwrap();
            }
            sum
        });
        (sum, runs.iter().all(|r| r.load(Ordering::SeqCst) == 1))
    });
    check!("the sum, and each ran once", (sum, runs), (49_999 * 50_000 / 2, true));
}

#[test]
fn panics_do_not_hurt_the_workers() {
    within(8, || {
        let rt = Runtime::new(3);
        let bad: Vec<_> = (0..30)
            .map(|i| {
                rt.spawn(async move {
                    if i >= 0 {
                        panic!("task {i}");
                    }
                })
            })
            .collect();
        rt.block_on(async {
            for (i, h) in bad.into_iter().enumerate() {
                assert_eq!(h.await, Err(JoinError::Panicked(format!("task {i}"))));
            }
        });
        let handles: Vec<_> = meet_up(3).into_iter().map(|f| rt.spawn(f)).collect();
        rt.block_on(async {
            for h in handles {
                h.await.unwrap();
            }
        });
    });
}

#[test]
fn abort_cancels_a_running_loop() {
    within(8, || {
        let rt = Runtime::new(2);
        let (gone, guard) = flag();
        let h = rt.spawn(async move {
            let _guard = guard;
            loop {
                yield_now().await;
            }
        });
        std::thread::sleep(ms(5));
        h.abort();
        let got: Result<(), JoinError> = rt.block_on(h);
        assert_eq!(got, Err(JoinError::Cancelled));
        assert!(gone.load(Ordering::SeqCst));
    });
}

#[test]
fn the_root_is_woken_from_any_thread_and_is_current() {
    within(8, || {
        let rt = Runtime::new(1);
        let set = Arc::new(AtomicBool::new(false));
        let s = set.clone();
        let polls = AtomicUsize::new(0);
        let local = std::cell::Cell::new(0);
        let got = rt.block_on(async {
            assert!(Handle::try_current().is_some(), "block_on must make the runtime current");
            let mut started = false;
            poll_fn(|cx| {
                polls.fetch_add(1, Ordering::SeqCst);
                if s.load(Ordering::SeqCst) {
                    return Poll::Ready(());
                }
                if !started {
                    started = true;
                    let (s, w) = (s.clone(), cx.waker().clone());
                    std::thread::spawn(move || {
                        std::thread::sleep(ms(30));
                        s.store(true, Ordering::SeqCst);
                        w.wake();
                    });
                }
                Poll::Pending
            })
            .await;
            local.set(spawn(async { 7 }).await.unwrap());
            local.get()
        });
        assert_eq!(got, 7);
        assert!(polls.load(Ordering::SeqCst) <= 3, "the root was polled {} times while waiting", polls.load(Ordering::SeqCst));
        assert!(Handle::try_current().is_none());
    });
}

#[test]
fn a_wake_during_the_root_poll_is_not_lost() {
    within(8, || {
        let rt = Runtime::new(1);
        let mut left = 2000;
        rt.block_on(poll_fn(move |cx| {
            if left == 0 {
                return Poll::Ready(());
            }
            left -= 1;
            cx.waker().wake_by_ref();
            Poll::Pending
        }));
    });
}

#[test]
fn many_sleeps_fire_and_none_early() {
    let early = within(8, || {
        let rt = Runtime::new(4);
        let handles: Vec<_> = (0..500u64)
            .map(|i| {
                rt.spawn(async move {
                    let wanted = Duration::from_micros(200 + (i * 7919) % 500 * 40);
                    let started = Instant::now();
                    sleep(wanted).await;
                    started.elapsed() < wanted
                })
            })
            .collect();
        rt.block_on(async {
            let mut early = 0;
            for h in handles {
                early += h.await.unwrap() as usize;
            }
            early
        })
    });
    check!("sleeps that ended early", early, 0);
}

#[test]
fn an_earlier_sleep_overtakes_a_much_later_one() {
    within(8, || {
        let rt = Runtime::new(2);
        rt.spawn(async { sleep(Duration::from_secs(3600)).await });
        std::thread::sleep(ms(30));
        // the timer thread is now waiting for the far deadline
        let short = rt.spawn(async {
            sleep(ms(10)).await;
            1
        });
        assert_eq!(rt.block_on(short), Ok(1));
        let late = rt.block_on(async { timeout(ms(10), sleep(Duration::from_secs(3600))).await });
        assert_eq!(late, Err(Elapsed));
        let ticks = rt.block_on(async {
            let started = now();
            let mut every = interval(ms(3));
            let mut seen = Vec::new();
            for _ in 0..4 {
                let due = every.tick().await;
                assert!(now() >= due, "a tick came early");
                seen.push(due);
            }
            assert!(seen[0] >= started);
            seen
        });
        assert_eq!(ticks.windows(2).map(|w| w[1] - w[0]).collect::<Vec<_>>(), vec![ms(3); 3]);
    });
}

#[test]
fn drop_does_not_wait_for_queued_or_sleeping_work() {
    within(8, || {
        let rt = Runtime::new(2);
        let mut gone = Vec::new();
        let mut handles = Vec::new();
        for kind in 0..6 {
            let (seen, guard) = flag();
            gone.push(seen);
            handles.push(rt.spawn(async move {
                let _guard = guard;
                if kind % 2 == 0 {
                    loop {
                        yield_now().await;
                    }
                } else {
                    sleep(Duration::from_secs(3600)).await;
                }
            }));
        }
        std::thread::sleep(ms(10));
        drop(rt);
        assert!(gone.iter().all(|g| g.load(Ordering::SeqCst)), "a future survived the runtime");
        for h in &mut handles {
            assert_eq!(poll_once(h), Poll::Ready(Err(JoinError::Cancelled)));
        }
    });
}

#[test]
fn drop_waits_for_the_poll_in_progress_and_destroys_the_queued() {
    within(8, || {
        let rt = Runtime::new(1);
        let (started_tx, started) = channel();
        let (release, wait) = channel::<()>();
        let done = Arc::new(AtomicBool::new(false));
        let d = done.clone();
        rt.spawn(async move {
            started_tx.send(()).unwrap();
            wait.recv().unwrap();
            std::thread::sleep(ms(30));
            d.store(true, Ordering::SeqCst);
        });
        started.recv().unwrap();
        // queued behind the blocked worker
        let queued: Vec<_> = (0..200)
            .map(|_| {
                let (seen, guard) = flag();
                (seen, rt.spawn(async move {
                    let _guard = guard;
                }))
            })
            .collect();
        release.send(()).unwrap();
        drop(rt);
        assert!(done.load(Ordering::SeqCst), "drop returned during a poll");
        for (seen, mut h) in queued {
            assert!(seen.load(Ordering::SeqCst), "a queued future survived");
            assert!(h.is_finished());
            assert!(poll_once(&mut h).is_ready());
        }
    });
}

#[test]
fn nothing_the_tasks_owned_survives_even_with_wakers_around() {
    within(8, || {
        let rt = Runtime::new(2);
        let owned = Arc::new(());
        let keep: Arc<Mutex<Vec<Waker>>> = Arc::default();
        for _ in 0..50 {
            let (o, k) = (owned.clone(), keep.clone());
            rt.spawn(async move {
                let _owned = o;
                poll_fn(|cx| {
                    k.lock().unwrap().push(cx.waker().clone());
                    Poll::<()>::Pending
                })
                .await;
            });
        }
        while keep.lock().unwrap().len() < 50 {
            std::thread::yield_now();
        }
        drop(rt);
        assert_eq!(Arc::strong_count(&owned), 1, "futures survived the runtime");
        // waking afterwards is harmless
        let wakers = std::mem::take(&mut *keep.lock().unwrap());
        wakers.into_iter().for_each(Waker::wake);
    });
}

#[test]
fn spawning_after_the_runtime_is_gone() {
    within(8, || {
        let rt = Runtime::new(1);
        let handle = rt.handle();
        drop(rt);
        let (gone, guard) = flag();
        let polled = Arc::new(AtomicBool::new(false));
        let p = polled.clone();
        let mut h = handle.spawn(async move {
            let _guard = guard;
            p.store(true, Ordering::SeqCst);
        });
        assert!(gone.load(Ordering::SeqCst), "the future was kept");
        assert!(!polled.load(Ordering::SeqCst));
        assert_eq!(poll_once(&mut h), Poll::Ready(Err(JoinError::Cancelled)));
    });
}

#[test]
fn destructors_see_the_runtime_and_may_spawn() {
    struct SpawnOnDrop {
        current: Arc<AtomicBool>,
        child_gone: Arc<AtomicBool>,
    }
    impl Drop for SpawnOnDrop {
        fn drop(&mut self) {
            self.current.store(Handle::try_current().is_some(), Ordering::SeqCst);
            let guard = DropFlag(self.child_gone.clone());
            if Handle::try_current().is_some() {
                drop(spawn(async move {
                    let _guard = guard;
                    std::future::pending::<()>().await;
                }));
            }
        }
    }
    let seen = within(8, || {
        let rt = Runtime::new(2);
        let current = Arc::new(AtomicBool::new(false));
        let child_gone = Arc::new(AtomicBool::new(false));
        let probe = SpawnOnDrop { current: current.clone(), child_gone: child_gone.clone() };
        let keep = Arc::new(Mutex::new(Vec::new()));
        let (_, guard) = flag();
        let k = keep.clone();
        rt.spawn(async move {
            let _probe = probe;
            parked(guard, k).await
        });
        while keep.lock().unwrap().is_empty() {
            std::thread::yield_now();
        }
        drop(rt);
        (current.load(Ordering::SeqCst), child_gone.load(Ordering::SeqCst))
    });
    check!("(current during the destructor, its child destroyed)", seen, (true, true));
}

#[test]
fn the_worker_threads_have_exited_after_drop() {
    struct OnExit(Arc<AtomicUsize>);
    impl Drop for OnExit {
        fn drop(&mut self) {
            std::thread::sleep(ms(20));
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    thread_local! {
        static SENTINEL: std::cell::RefCell<Option<OnExit>> = const { std::cell::RefCell::new(None) };
    }
    let exited = within(8, || {
        let rt = Runtime::new(4);
        let exits = Arc::new(AtomicUsize::new(0));
        let barrier = Arc::new(Barrier::new(4));
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let (exits, barrier) = (exits.clone(), barrier.clone());
                rt.spawn(async move {
                    SENTINEL.with(|s| *s.borrow_mut() = Some(OnExit(exits)));
                    barrier.wait();
                })
            })
            .collect();
        rt.block_on(async {
            for h in handles {
                h.await.unwrap();
            }
        });
        drop(rt);
        exits.load(Ordering::SeqCst)
    });
    check!("workers that had run their exit code when drop returned", exited, 4);
}

#[test]
fn drop_while_other_threads_spawn_and_wake() {
    within(12, || {
        for _ in 0..40 {
            let rt = Runtime::new(3);
            let handle = rt.handle();
            let owned = Arc::new(());
            let keep: Arc<Mutex<Vec<Waker>>> = Arc::default();
            let stop = Arc::new(AtomicBool::new(false));
            let (o, k, s) = (owned.clone(), keep.clone(), stop.clone());
            let outsider = std::thread::spawn(move || {
                let mut joins = Vec::new();
                while !s.load(Ordering::SeqCst) && joins.len() < 2000 {
                    let (o, k2) = (o.clone(), k.clone());
                    joins.push(handle.spawn(async move {
                        let _owned = o;
                        poll_fn(|cx| {
                            k2.lock().unwrap().push(cx.waker().clone());
                            Poll::<()>::Pending
                        })
                        .await;
                    }));
                    let some = k.lock().unwrap().pop();
                    if let Some(w) = some {
                        w.wake();
                    }
                }
                joins
            });
            std::thread::sleep(Duration::from_micros(300));
            drop(rt);
            stop.store(true, Ordering::SeqCst);
            for mut h in outsider.join().unwrap() {
                assert_eq!(poll_once(&mut h), Poll::Ready(Err(JoinError::Cancelled)));
            }
            assert_eq!(Arc::strong_count(&owned), 1, "a future survived");
        }
    });
}

/// Threads outside the runtime wake tasks as fast as they can. No task may be polled by two workers at once.
#[test]
fn hammered_tasks_are_never_polled_twice_at_once() {
    let overlaps = within(12, || {
        let rt = Runtime::new(4);
        let overlaps = Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::new();
        let mut hammers = Vec::new();
        for _ in 0..100 {
            let count = Arc::new(AtomicUsize::new(0));
            let slot: Arc<Mutex<Option<Waker>>> = Arc::default();
            let busy = Arc::new(AtomicBool::new(false));
            let (c, s, b, o) = (count.clone(), slot.clone(), busy.clone(), overlaps.clone());
            handles.push(rt.spawn(poll_fn(move |cx| {
                if b.swap(true, Ordering::SeqCst) {
                    o.fetch_add(1, Ordering::SeqCst);
                }
                *s.lock().unwrap() = Some(cx.waker().clone());
                let done = c.load(Ordering::SeqCst) >= 200;
                b.store(false, Ordering::SeqCst);
                if done {
                    Poll::Ready(())
                } else {
                    Poll::Pending
                }
            })));
            hammers.push((count, slot));
        }
        let hammers = Arc::new(hammers);
        let threads: Vec<_> = (0..2)
            .map(|_| {
                let hammers = hammers.clone();
                std::thread::spawn(move || {
                    for _ in 0..100 {
                        for (count, slot) in hammers.iter() {
                            count.fetch_add(1, Ordering::SeqCst);
                            let waker = slot.lock().unwrap().clone();
                            if let Some(w) = waker {
                                w.wake();
                            }
                        }
                    }
                })
            })
            .collect();
        threads.into_iter().for_each(|t| t.join().unwrap());
        rt.block_on(async {
            for h in handles {
                h.await.unwrap();
            }
        });
        overlaps.load(Ordering::SeqCst)
    });
    check!("overlapping polls", overlaps, 0);
}
