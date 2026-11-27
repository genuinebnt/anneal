use solution::*;

#[allow(dead_code, unused_imports)]
use std::collections::{HashSet, VecDeque};
#[allow(dead_code, unused_imports)]
use std::sync::atomic::{AtomicUsize, Ordering};
#[allow(dead_code, unused_imports)]
use std::sync::{Arc, Mutex};

#[allow(dead_code)]
type Log = Arc<Mutex<Vec<u32>>>;

/// Schedules a task, from wherever the caller is, that writes `id` down when run.
#[allow(dead_code)]
fn add(pool: &Arc<Pool>, log: &Log, id: u32) {
    let log = log.clone();
    let scheduler: Arc<dyn Schedule> = pool.clone();
    drop(spawn_on(&scheduler, async move { log.lock().unwrap().push(id) }));
}

/// Adds from a thread that has entered no worker.
#[allow(dead_code)]
fn add_outside(pool: &Arc<Pool>, log: &Log, id: u32) {
    std::thread::scope(|s| {
        s.spawn(|| add(pool, log, id));
    });
}

/// Takes one task, runs it, and returns its id.
#[allow(dead_code)]
fn take(worker: &mut EnteredWorker<'_>, log: &Log) -> Option<u32> {
    let task = worker.next_task()?;
    task.run();
    log.lock().unwrap().pop()
}

#[allow(dead_code)]
fn drain(worker: &mut EnteredWorker<'_>, log: &Log) -> Vec<u32> {
    std::iter::from_fn(|| take(worker, log)).collect()
}

#[test]
fn outside_work_goes_to_the_shared_queue() {
    let (pool, mut workers) = pool(2);
    let log = Log::default();
    add(&pool, &log, 1);
    add(&pool, &log, 2);
    check!("has work", pool.has_work(), true);
    let mut w = workers[0].enter();
    check!("oldest first", drain(&mut w, &log), vec![1, 2]);
    check!("no more", pool.has_work(), false);
}

#[test]
fn own_work_comes_before_shared_work() {
    let (pool, mut workers) = pool(1);
    let log = Log::default();
    add(&pool, &log, 100); // shared, and older
    let mut w = workers[0].enter();
    add(&pool, &log, 1); // own
    add(&pool, &log, 2);
    check!("own queue first, in order", drain(&mut w, &log), vec![1, 2, 100]);
}

#[test]
fn every_61st_look_takes_shared_work_first() {
    let (pool, mut workers) = pool(1);
    let log = Log::default();
    let mut w = workers[0].enter();
    for id in 0..100 {
        add(&pool, &log, id);
    }
    add_outside(&pool, &log, 1000);
    let got = drain(&mut w, &log);
    check!("the 61st task taken", got.get(60).copied(), Some(1000));
    check!("all 101", got.len(), 101);
}

#[test]
fn an_idle_worker_takes_from_another() {
    let (pool, mut workers) = pool(2);
    let log = Log::default();
    {
        let _w0 = workers[0].enter();
        for id in 0..10 {
            add(&pool, &log, id);
        }
    }
    let mut w1 = workers[1].enter();
    let mut got = drain(&mut w1, &log);
    got.sort();
    check!("all of them", got, (0..10).collect::<Vec<_>>());
    assert!(pool.steals(1) >= 1, "no steal was counted");
    check!("worker 0 stole nothing", pool.steals(0), 0);
}

#[test]
fn another_pools_worker_does_not_count() {
    let (a, mut workers_a) = pool(1);
    let (b, mut workers_b) = pool(1);
    let log = Log::default();
    {
        let _in_a = workers_a[0].enter();
        add(&b, &log, 7); // b's shared queue
        add(&a, &log, 8); // a's own queue
    }
    let mut w = workers_b[0].enter();
    check!("b has it", drain(&mut w, &log), vec![7]);
    check!("no stealing was needed", b.steals(0), 0);
}

#[test]
#[should_panic]
fn zero_workers_panics() {
    let _ = pool(0);
}
