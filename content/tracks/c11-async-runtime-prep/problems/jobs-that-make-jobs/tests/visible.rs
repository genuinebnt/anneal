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
fn every_given_job_is_handled() {
    within(10, || {
        let done = Mutex::new(Vec::new());
        let report = run(3, (0..20).collect(), |job: u32, _| done.lock().unwrap().push(job));
        let mut seen = done.into_inner().unwrap();
        seen.sort();
        check!("each once", seen, (0..20).collect::<Vec<_>>());
        check!("report", (report.per_worker.len(), report.per_worker.iter().sum::<usize>()), (3, 20));
    });
}

#[test]
fn spawned_jobs_are_handled_too() {
    within(10, || {
        let count = AtomicUsize::new(0);
        run(2, vec![6u32], |depth, spawner| {
            count.fetch_add(1, Ordering::SeqCst);
            if depth > 0 {
                spawner.spawn(depth - 1);
                spawner.spawn(depth - 1);
            }
        });
        check!("a full binary tree of depth 6", count.load(Ordering::SeqCst), 127);
    });
}

#[test]
fn nothing_to_do_returns() {
    within(10, || {
        let report = run(4, Vec::<u8>::new(), |_, _| {});
        check!("no jobs", report.per_worker, vec![0; 4]);
    });
}

#[test]
fn own_work_comes_first() {
    within(10, || {
        let order = Mutex::new(Vec::new());
        run(1, vec!["a", "b"], |job, spawner| {
            order.lock().unwrap().push(job);
            if job == "a" {
                spawner.spawn("a1");
                spawner.spawn("a2");
            }
        });
        let order = order.into_inner().unwrap();
        check!("a's jobs run before b", order.last().copied(), Some("b"));
        check!("all ran", order.len(), 4);
    });
}

#[test]
fn spawned_jobs_are_shared_out() {
    // one job spawns three more, then all four wait for each other: only possible if four workers run at once
    within(10, || {
        let meet = Barrier::new(4);
        run(4, vec![0u32], |job, spawner| {
            if job == 0 {
                for child in 1..4 {
                    spawner.spawn(child);
                }
            }
            meet.wait();
        });
    });
}

#[test]
#[should_panic]
fn zero_workers_panics() {
    run(0, vec![1], |_: u8, _| {});
}
