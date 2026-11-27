use solution::*;

#[allow(dead_code, unused_imports)]
use std::panic::{catch_unwind, AssertUnwindSafe};
#[allow(dead_code, unused_imports)]
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
#[allow(dead_code, unused_imports)]
use std::sync::mpsc::{channel, RecvTimeoutError};
#[allow(dead_code, unused_imports)]
use std::sync::{Arc, Barrier};

#[allow(dead_code)]
fn panics(f: impl FnOnce()) -> bool {
    catch_unwind(AssertUnwindSafe(f)).is_err()
}

#[test]
#[should_panic]
fn begin_twice_panics() {
    let job = QueueState::new();
    job.request();
    job.begin();
    job.begin();
}

#[test]
#[should_panic]
fn end_without_a_run_panics() {
    let job = QueueState::new();
    job.request();
    job.end();
}

#[test]
#[should_panic]
fn finish_on_a_queued_job_panics() {
    let job = QueueState::new();
    job.request();
    job.finish();
}

#[test]
#[should_panic]
fn a_finished_job_cannot_begin() {
    let job = QueueState::new();
    job.request();
    job.begin();
    job.finish();
    job.request();
    job.begin();
}

#[test]
fn a_panicking_call_changes_nothing() {
    let job = QueueState::new();
    check!("end on idle", panics(|| {
        job.end();
    }), true);
    check!("finish on idle", panics(|| job.finish()), true);
    check!("still idle", job.request(), true);
    check!("end while queued", panics(|| {
        job.end();
    }), true);
    check!("still queued", job.request(), false);
    job.begin();
    check!("begin while running", panics(|| job.begin()), true);
    check!("still running with no request", job.end(), false);
}

#[test]
fn end_after_a_request_before_begin_is_false() {
    let job = QueueState::new();
    job.request();
    job.request();
    job.begin();
    check!("the requests came before the run began, so that run covers them", job.end(), false);
}

#[test]
fn random_calls_match_a_model() {
    #[derive(Clone, Copy, Debug, PartialEq)]
    enum M {
        Idle,
        Queued,
        Running(bool),
        Done,
    }
    let mut rng = anneal_prelude::Rng::new(110_401);
    for round in 0..2000 {
        let job = QueueState::new();
        let mut m = M::Idle;
        let mut history = Vec::new();
        for _ in 0..30 {
            match rng.below(if round % 4 == 0 { 8 } else { 7 }) {
                0..=2 => {
                    history.push("request");
                    let want = m == M::Idle;
                    check!(format!("{history:?}"), job.request(), want);
                    m = match m {
                        M::Idle => M::Queued,
                        M::Running(_) => M::Running(true),
                        other => other,
                    };
                }
                3 | 4 if m == M::Queued => {
                    history.push("begin");
                    job.begin();
                    m = M::Running(false);
                }
                5 | 6 => {
                    if let M::Running(again) = m {
                        history.push("end");
                        check!(format!("{history:?}"), job.end(), again);
                        m = if again { M::Queued } else { M::Idle };
                    }
                }
                7 => {
                    if matches!(m, M::Running(_)) {
                        history.push("finish");
                        job.finish();
                        m = M::Done;
                    }
                }
                _ => {}
            }
        }
    }
}

/// Producers add to a counter and then request. One worker serves a real queue. The job must never be in the queue twice,
/// and the last run must see every addition.
#[test]
fn many_producers_one_job() {
    const PRODUCERS: usize = 6;
    const EACH: u64 = 3000;
    for _ in 0..8 {
        let job = Arc::new(QueueState::new());
        let work = Arc::new(AtomicU64::new(0));
        let in_queue = Arc::new(AtomicUsize::new(0));
        let (queue, tokens) = channel::<()>();
        let start = Arc::new(Barrier::new(PRODUCERS));
        let producers: Vec<_> = (0..PRODUCERS)
            .map(|_| {
                let (job, work, in_queue, queue, start) = (job.clone(), work.clone(), in_queue.clone(), queue.clone(), start.clone());
                std::thread::spawn(move || {
                    start.wait();
                    for i in 0..EACH {
                        work.fetch_add(1, Ordering::Relaxed);
                        if job.request() {
                            assert_eq!(in_queue.fetch_add(1, Ordering::SeqCst), 0, "queued twice");
                            queue.send(()).unwrap();
                        }
                        if i % 64 == 0 {
                            std::thread::yield_now();
                        }
                    }
                })
            })
            .collect();
        let mut seen = 0;
        let mut runs = 0;
        loop {
            // read before waiting: once they have all finished, only this thread can still queue the job
            let finished = producers.iter().all(|p| p.is_finished());
            match tokens.recv_timeout(std::time::Duration::from_millis(5)) {
                Ok(()) => {
                    assert_eq!(in_queue.fetch_sub(1, Ordering::SeqCst), 1, "queued twice");
                    runs += 1;
                    assert!(runs <= PRODUCERS as u64 * EACH, "more runs than requests: the job is queued again without a new request");
                    job.begin();
                    seen = work.load(Ordering::Relaxed);
                    std::thread::yield_now();
                    if job.end() {
                        assert_eq!(in_queue.fetch_add(1, Ordering::SeqCst), 0, "queued twice");
                        queue.send(()).unwrap();
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    if finished {
                        break;
                    }
                }
                Err(RecvTimeoutError::Disconnected) => unreachable!("this thread holds a sender"),
            }
        }
        producers.into_iter().for_each(|p| p.join().unwrap());
        check!("times the job still counts as queued", in_queue.load(Ordering::SeqCst), 0);
        check!("the last run saw all the work", seen, PRODUCERS as u64 * EACH);
    }
}

#[test]
fn a_request_racing_the_end_of_a_run_is_not_lost() {
    for round in 0..3000 {
        let job = Arc::new(QueueState::new());
        job.request();
        job.begin();
        let go = Arc::new(Barrier::new(2));
        let (j, g) = (job.clone(), go.clone());
        let other = std::thread::spawn(move || {
            g.wait();
            j.request()
        });
        go.wait();
        let again = job.end();
        let must_queue = other.join().unwrap();
        // exactly one of the two callers must be told to queue the job
        check!(format!("round {round}: (end, request)"), again ^ must_queue, true);
    }
}

#[test]
fn it_is_shared_between_threads() {
    fn both<T: Send + Sync>() {}
    both::<QueueState>();
}
