use solution::*;

fn within<T: Send + 'static>(ms: u64, f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(std::time::Duration::from_millis(ms)).ok()
}

        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::{Arc, Barrier, Mutex};
        use std::time::Duration;

        #[test]
        fn zero_workers_panics() {
            assert!(std::panic::catch_unwind(|| ThreadPool::new(0)).is_err());
        }

        #[test]
        fn worker_survives_a_panicking_job() {
            let p = ThreadPool::new(1);
            let bad = p.submit(|| -> u8 { panic!("job panic") });
            let good = p.submit(|| 5u8);
            check!("the panicking task", within(3000, move || bad.join()), Some(Err(TaskPanicked)));
            check!("the only worker is still alive for the next job", within(3000, move || good.join()), Some(Ok(5)));
        }

        #[test]
        fn pool_keeps_its_size_after_many_panics() {
            let p = ThreadPool::new(2);
            for _ in 0..6 {
                p.execute(|| panic!("again"));
            }
            // two workers must still be able to run two jobs at the same time
            let barrier = Arc::new(Barrier::new(2));
            let hs: Vec<_> = (0..2).map(|_| { let b = barrier.clone(); p.submit(move || { b.wait(); 1 }) }).collect();
            let r = within(3000, move || hs.into_iter().map(|h| h.join().unwrap()).sum::<i32>());
            check!("two rendezvousing jobs after six panics", r, Some(2));
            check!("shutdown reports the panics", p.shutdown(), 6);
        }

        #[test]
        fn jobs_really_run_in_parallel() {
            let p = ThreadPool::new(4);
            let barrier = Arc::new(Barrier::new(4));
            let hs: Vec<_> = (0..4).map(|_| { let b = barrier.clone(); p.submit(move || { b.wait(); 1 }) }).collect();
            let r = within(3000, move || hs.into_iter().map(|h| h.join().unwrap()).sum::<i32>());
            check!("4 jobs that wait for each other on 4 workers", r, Some(4));
        }

        #[test]
        fn never_more_threads_than_workers() {
            let ids = Arc::new(Mutex::new(std::collections::HashSet::new()));
            let p = ThreadPool::new(3);
            for _ in 0..200 {
                let ids = ids.clone();
                p.execute(move || {
                    ids.lock().unwrap().insert(std::thread::current().id());
                    std::thread::sleep(Duration::from_micros(200));
                });
            }
            p.shutdown();
            let n = ids.lock().unwrap().len();
            check!("distinct threads that ran 200 jobs (1 to 3)", (1..=3).contains(&n), true);
        }

        #[test]
        fn shutdown_runs_every_queued_job() {
            let done = Arc::new(AtomicUsize::new(0));
            let p = ThreadPool::new(2);
            for _ in 0..60 {
                let done = done.clone();
                p.execute(move || {
                    std::thread::sleep(Duration::from_millis(2));
                    done.fetch_add(1, Ordering::SeqCst);
                });
            }
            let panics = p.shutdown(); // most jobs are still queued right now
            check!("jobs finished and panics", (done.load(Ordering::SeqCst), panics), (60, 0));
        }

        #[test]
        fn drop_joins_the_workers_and_finishes_the_queue() {
            let done = Arc::new(AtomicUsize::new(0));
            {
                let p = ThreadPool::new(2);
                for _ in 0..10 {
                    let done = done.clone();
                    p.execute(move || {
                        std::thread::sleep(Duration::from_millis(20));
                        done.fetch_add(1, Ordering::SeqCst);
                    });
                }
            } // dropped here
            check!("all ten finished by the time the pool is gone", done.load(Ordering::SeqCst), 10);
        }

        #[test]
        fn results_come_back_to_the_right_handle() {
            let p = ThreadPool::new(4);
            let hs: Vec<_> = (0..200u64).map(|i| p.submit(move || { std::thread::sleep(Duration::from_micros(50 * (i % 5))); i * i })).collect();
            let got: Vec<u64> = hs.into_iter().map(|h| h.join().unwrap()).collect();
            check!("200 tasks with mixed durations: squares in handle order", got, (0..200u64).map(|i| i * i).collect::<Vec<_>>());
        }

        #[test]
        fn executing_from_several_threads_at_once() {
            let p = Arc::new(ThreadPool::new(3));
            let total = Arc::new(AtomicUsize::new(0));
            let hs: Vec<_> = (0..4)
                .map(|_| {
                    let (p, total) = (p.clone(), total.clone());
                    std::thread::spawn(move || {
                        for _ in 0..2_500 {
                            let total = total.clone();
                            p.execute(move || { total.fetch_add(1, Ordering::SeqCst); });
                        }
                    })
                })
                .collect();
            for h in hs {
                h.join().unwrap();
            }
            let p = Arc::try_unwrap(p).ok().expect("pool still shared");
            p.shutdown();
            check!("4 submitting threads x 2500 jobs", total.load(Ordering::SeqCst), 10_000);
        }
