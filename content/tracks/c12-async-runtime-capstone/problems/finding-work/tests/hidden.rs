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
fn the_count_goes_on_across_enters_and_includes_empty_looks() {
    let (pool, mut workers) = pool(1);
    let log = Log::default();
    for _ in 0..4 {
        let mut w = workers[0].enter();
        for _ in 0..15 {
            check!("empty", w.next_task().is_none(), true);
        }
    }
    // 60 looks so far: the next is the 61st
    let mut w = workers[0].enter();
    add(&pool, &log, 1);
    add_outside(&pool, &log, 2);
    check!("61st: shared first", take(&mut w, &log), Some(2));
    check!("62nd: own", take(&mut w, &log), Some(1));
}

#[test]
fn the_122nd_look_is_special_too() {
    let (pool, mut workers) = pool(1);
    let log = Log::default();
    let mut w = workers[0].enter();
    for id in 0..200 {
        add(&pool, &log, id);
    }
    add_outside(&pool, &log, 1000);
    let mut got: Vec<u32> = (0..61).filter_map(|_| take(&mut w, &log)).collect();
    // added only now, so that no earlier look could have moved it along with the first one
    add_outside(&pool, &log, 1001);
    got.extend(drain(&mut w, &log));
    check!("61st and 122nd", (got[60], got[121]), (1000, 1001));
    let own: Vec<u32> = got.iter().copied().filter(|&id| id < 1000).collect();
    check!("own tasks stay in order", own, (0..200).collect::<Vec<_>>());
}

#[test]
fn an_empty_shared_queue_on_the_61st_look_falls_back_to_own_work() {
    let (pool, mut workers) = pool(1);
    let log = Log::default();
    let mut w = workers[0].enter();
    for id in 0..70 {
        add(&pool, &log, id);
    }
    check!("nothing changes", drain(&mut w, &log), (0..70).collect::<Vec<_>>());
}

#[test]
fn without_a_special_look_own_work_wins_for_a_long_time() {
    let (pool, mut workers) = pool(1);
    let log = Log::default();
    add(&pool, &log, 1000);
    let mut w = workers[0].enter();
    for id in 0..60 {
        add(&pool, &log, id);
    }
    check!("the first 60 looks take own tasks", (0..60).filter_map(|_| take(&mut w, &log)).collect::<Vec<_>>(), (0..60).collect::<Vec<_>>());
    check!("then the outside one", take(&mut w, &log), Some(1000));
}

#[test]
fn shared_work_comes_before_stealing() {
    let (pool, mut workers) = pool(2);
    let log = Log::default();
    {
        let _w0 = workers[0].enter();
        add(&pool, &log, 1);
    }
    add(&pool, &log, 2);
    let mut w1 = workers[1].enter();
    check!("shared first", take(&mut w1, &log), Some(2));
    check!("not a steal", pool.steals(1), 0);
    check!("then steal", take(&mut w1, &log), Some(1));
    check!("one steal", pool.steals(1), 1);
}

#[test]
fn nested_enters_use_the_innermost() {
    let (a, mut wa) = pool(2);
    let (b, mut wb) = pool(1);
    let log = Log::default();
    let (first, second) = wa.split_at_mut(1);
    let outer = first[0].enter();
    let inner = second[0].enter();
    add(&a, &log, 1); // worker 1's queue
    let foreign = wb[0].enter();
    add(&a, &log, 2); // the innermost is b's: shared
    drop(foreign);
    drop(outer); // out of order
    add(&a, &log, 3); // still worker 1
    check!("worker 1 has its own two tasks", inner.has_local_tasks(), true);
    drop(inner);
    add(&a, &log, 4); // nobody entered: shared
    let mut w = second[0].enter();
    check!("own 1 and 3 first, then shared 2 and 4", drain(&mut w, &log), vec![1, 3, 2, 4]);
    drop(w);
    let _ = b;
}

#[test]
fn the_queue_moves_with_the_worker() {
    let (pool, mut workers) = pool(1);
    let log = Log::default();
    {
        let _w = workers[0].enter();
        add(&pool, &log, 5);
    }
    let mut worker = workers.pop().unwrap();
    check!("index", worker.index(), 0);
    let l = log.clone();
    let got = std::thread::spawn(move || {
        let mut w = worker.enter();
        drain(&mut w, &l)
    })
    .join()
    .unwrap();
    check!("on another thread", got, vec![5]);
}

#[test]
fn has_local_tasks_and_has_work() {
    let (pool, mut workers) = pool(2);
    let log = Log::default();
    check!("empty", pool.has_work(), false);
    let mut w0 = workers[0].enter();
    check!("none", w0.has_local_tasks(), false);
    add(&pool, &log, 1);
    check!("own", (w0.has_local_tasks(), pool.has_work()), (true, true));
    take(&mut w0, &log);
    check!("drained", (w0.has_local_tasks(), pool.has_work()), (false, false));
    add_outside(&pool, &log, 2);
    check!("shared", (w0.has_local_tasks(), pool.has_work()), (false, true));
}

#[test]
fn random_traffic_loses_and_repeats_nothing() {
    let mut rng = anneal_prelude::Rng::new(120_501);
    for round in 0..300 {
        let n = 1 + rng.below(4);
        let (pool, mut workers) = pool(n);
        let log = Log::default();
        let mut waiting = HashSet::new();
        let mut next = 0u32;
        let mut history = Vec::new();
        for _ in 0..80 {
            let who = rng.below(n);
            match rng.below(5) {
                0 => {
                    add_outside(&pool, &log, next);
                    history.push(format!("outside {next}"));
                    waiting.insert(next);
                    next += 1;
                }
                1 | 2 => {
                    let _w = workers[who].enter();
                    for _ in 0..rng.below(5) {
                        add(&pool, &log, next);
                        history.push(format!("w{who} schedules {next}"));
                        waiting.insert(next);
                        next += 1;
                    }
                }
                _ => {
                    let mut w = workers[who].enter();
                    let got = take(&mut w, &log);
                    history.push(format!("w{who} takes {got:?}"));
                    match got {
                        Some(id) => assert!(waiting.remove(&id), "round {round}: {id} returned twice {history:?}"),
                        None => check!(format!("round {round} {history:?}: None although tasks wait"), waiting.len(), 0),
                    }
                }
            }
            check!(format!("round {round} {history:?}: has_work"), pool.has_work(), !waiting.is_empty());
        }
    }
}

#[test]
fn four_threads_run_a_tree_of_tasks() {
    let (pool, workers) = pool(4);
    const TOTAL: usize = 20_000;
    let runs: Arc<Vec<AtomicUsize>> = Arc::new((0..TOTAL).map(|_| AtomicUsize::new(0)).collect());
    let done = Arc::new(AtomicUsize::new(0));
    fn node(pool: Arc<Pool>, runs: Arc<Vec<AtomicUsize>>, done: Arc<AtomicUsize>, id: usize) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
        Box::pin(async move {
            runs[id].fetch_add(1, Ordering::SeqCst);
            for child in [id * 2 + 2, id * 2 + 3] {
                if child < runs.len() {
                    let scheduler: Arc<dyn Schedule> = pool.clone();
                    drop(spawn_on(&scheduler, node(pool.clone(), runs.clone(), done.clone(), child)));
                }
            }
            done.fetch_add(1, Ordering::SeqCst);
        })
    }
    let threads: Vec<_> = workers
        .into_iter()
        .map(|mut worker| {
            let done = done.clone();
            std::thread::spawn(move || {
                let mut w = worker.enter();
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
                while done.load(Ordering::SeqCst) < TOTAL {
                    match w.next_task() {
                        Some(task) => task.run(),
                        None => std::thread::yield_now(),
                    }
                    assert!(std::time::Instant::now() < deadline, "tasks went missing");
                }
            })
        })
        .collect();
    // two roots, from outside, while the workers are already looking
    for root in [0, 1] {
        let scheduler: Arc<dyn Schedule> = pool.clone();
        drop(spawn_on(&scheduler, node(pool.clone(), runs.clone(), done.clone(), root)));
    }
    threads.into_iter().for_each(|t| t.join().unwrap());
    check!("every task ran once", runs.iter().filter(|r| r.load(Ordering::SeqCst) != 1).count(), 0);
    check!("queues empty", pool.has_work(), false);
}

#[test]
fn types_cross_threads() {
    fn both<T: Send + Sync>() {}
    fn send<T: Send>() {}
    both::<Pool>();
    send::<LocalWorker>();
}
