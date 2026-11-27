use solution::*;

#[allow(dead_code, unused_imports)]
use std::panic::{catch_unwind, AssertUnwindSafe};
#[allow(dead_code, unused_imports)]
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
#[allow(dead_code, unused_imports)]
use std::sync::{Arc, Barrier, Mutex};

/// Turns a hang into a failure.
#[allow(dead_code)]
fn within<T: Send + 'static>(seconds: u64, f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = std::sync::mpsc::channel();
    let thread = std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    match rx.recv_timeout(std::time::Duration::from_secs(seconds)) {
        Ok(value) => value,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => panic!("run() hung"),
        Err(_) => std::panic::resume_unwind(thread.join().expect_err("it sent nothing")),
    }
}

#[test]
fn workers_busy_with_long_jobs_do_not_block_the_queue() {
    // every worker count from 2 to 8: all of them must be able to hold a job at the same time
    for workers in 2..=8 {
        within(10, move || {
            let meet = Barrier::new(workers);
            run(workers, vec![0usize], |job, spawner| {
                if job == 0 {
                    for child in 1..workers {
                        spawner.spawn(child);
                    }
                }
                meet.wait();
            });
        });
    }
}

#[test]
fn jobs_spawned_two_levels_down_are_shared_too() {
    within(10, || {
        let meet = Barrier::new(3);
        run(3, vec!["root"], |job, spawner| match job {
            "root" => spawner.spawn("mid"),
            "mid" => {
                spawner.spawn("leaf1");
                spawner.spawn("leaf2");
                meet.wait();
            }
            _ => {
                meet.wait();
            }
        });
    });
}

#[test]
fn a_late_spawn_wakes_up_the_others() {
    // the other workers have had nothing to do for a while when the jobs appear
    within(10, || {
        let meet = Barrier::new(4);
        run(4, vec![0u8], |job, spawner| {
            if job == 0 {
                std::thread::sleep(std::time::Duration::from_millis(30));
                (1..4).for_each(|c| spawner.spawn(c));
            }
            meet.wait();
        });
    });
}

#[test]
fn own_work_first_through_many_levels() {
    within(10, || {
        let order = Mutex::new(Vec::new());
        run(1, vec![(1u32, 3u32), (2, 3), (3, 3)], |(root, depth), spawner| {
            order.lock().unwrap().push(root);
            if depth > 0 {
                spawner.spawn((root, depth - 1));
                spawner.spawn((root, depth - 1));
            }
        });
        let order = order.into_inner().unwrap();
        let mut grouped = order.clone();
        grouped.dedup();
        check!("each given job's family runs in one block", grouped, vec![1, 2, 3]);
        check!("15 jobs per family", order.len(), 45);
    });
}

#[test]
fn random_trees_run_each_job_once() {
    within(10, || {
        let mut rng = anneal_prelude::Rng::new(110_901);
        for round in 0..60 {
            let n = 1 + rng.below(300);
            // job i > 0 is spawned by a job with a smaller number; some jobs are given instead
            let mut children = vec![Vec::new(); n];
            let mut given = vec![0];
            for i in 1..n {
                if rng.below(8) == 0 {
                    given.push(i);
                } else {
                    let parent = rng.below(i);
                    children[parent].push(i);
                }
            }
            let hits: Vec<AtomicU32> = (0..n).map(|_| AtomicU32::new(0)).collect();
            let workers = 1 + rng.below(6);
            let report = run(workers, given, |job: usize, spawner| {
                assert!(spawner.worker() < workers);
                hits[job].fetch_add(1, Ordering::SeqCst);
                for &child in &children[job] {
                    spawner.spawn(child);
                }
            });
            let wrong: Vec<usize> = (0..n).filter(|&i| hits[i].load(Ordering::SeqCst) != 1).collect();
            check!(format!("round {round}: jobs not handled exactly once"), wrong, Vec::<usize>::new());
            check!(format!("round {round}: report total"), report.per_worker.iter().sum::<usize>(), n);
        }
    });
}

#[test]
fn the_report_matches_who_ran_what() {
    within(10, || {
        let by_worker: Vec<AtomicUsize> = (0..4).map(|_| AtomicUsize::new(0)).collect();
        let report = run(4, (0..2000u32).collect(), |_, spawner| {
            by_worker[spawner.worker()].fetch_add(1, Ordering::SeqCst);
        });
        check!("per worker", report.per_worker, by_worker.iter().map(|c| c.load(Ordering::SeqCst)).collect::<Vec<_>>());
    });
}

#[test]
fn a_long_chain_does_not_grow_the_stack() {
    within(10, || {
        let count = AtomicUsize::new(0);
        run(2, vec![200_000u32], |left, spawner| {
            count.fetch_add(1, Ordering::SeqCst);
            if left > 0 {
                spawner.spawn(left - 1);
            }
        });
        check!("200001 jobs", count.load(Ordering::SeqCst), 200_001);
    });
}

#[test]
fn a_wide_fan_of_jobs() {
    within(10, || {
        let count = AtomicUsize::new(0);
        run(4, vec![true], |root, spawner| {
            count.fetch_add(1, Ordering::SeqCst);
            if root {
                (0..100_000).for_each(|_| spawner.spawn(false));
            }
        });
        check!("all of them", count.load(Ordering::SeqCst), 100_001);
    });
}

#[test]
fn a_panic_comes_out_and_nothing_hangs() {
    for workers in [1, 2, 4] {
        within(10, move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                run(workers, (0..200u32).collect(), |job, spawner| {
                    if job == 13 {
                        panic!("job 13");
                    }
                    if job < 100 {
                        spawner.spawn(job + 1000);
                    }
                })
            }));
            assert!(result.is_err(), "run returned normally although a handler panicked");
        });
    }
}

#[test]
fn every_job_is_dropped_when_a_handler_panics() {
    struct Job(u32, Arc<AtomicUsize>);
    impl Drop for Job {
        fn drop(&mut self) {
            self.1.fetch_add(1, Ordering::SeqCst);
        }
    }
    let drops = Arc::new(AtomicUsize::new(0));
    let d = drops.clone();
    let result = within(10, move || {
        catch_unwind(AssertUnwindSafe(|| {
            let jobs = (0..50).map(|i| Job(i, d.clone())).collect();
            run(3, jobs, |job: Job, spawner| {
                if job.0 < 100 {
                    spawner.spawn(Job(job.0 + 100, job.1.clone()));
                }
                if job.0 == 7 {
                    panic!("seven");
                }
            })
        }))
        .is_err()
    });
    check!("panicked", result, true);
    // 50 given, plus one spawned by every given job that was handled
    assert!(drops.load(Ordering::SeqCst) >= 51, "only {} jobs were dropped", drops.load(Ordering::SeqCst));
    check!("no job is still alive", Arc::strong_count(&drops), 1);
}

#[test]
fn borrowed_data_is_allowed() {
    within(10, || {
        let words = ["one".to_string(), "two".to_string()];
        let seen = Mutex::new(0);
        run(2, words.iter().collect(), |w: &String, _| *seen.lock().unwrap() += w.len());
        check!("jobs and handler may borrow", *seen.lock().unwrap(), 6);
    });
}
