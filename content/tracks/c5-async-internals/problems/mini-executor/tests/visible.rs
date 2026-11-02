use solution::*;

#[test]
fn runs_a_task() {
    let ex = Executor::new();
    let h = ex.spawn(async { 21 * 2 });
    check!("not finished before run", h.is_finished(), false);
    ex.run();
    check!("output after run", h.take(), Some(42));
}

#[test]
fn run_with_nothing_to_do_returns() {
    let ex = Executor::new();
    ex.run();
    ex.run();
}

#[test]
fn yielding_tasks_take_turns() {
    use std::sync::{Arc, Mutex};
    let ex = Executor::new();
    let log = Arc::new(Mutex::new(Vec::new()));
    for id in 0..2u32 {
        let log = log.clone();
        ex.spawn(async move {
            for step in 0..3u32 {
                log.lock().unwrap().push((id, step));
                yield_now().await;
            }
        });
    }
    ex.run();
    check!("interleaved in FIFO order", log.lock().unwrap().clone(), vec![(0, 0), (1, 0), (0, 1), (1, 1), (0, 2), (1, 2)]);
}

#[test]
fn a_task_awaits_another_through_its_join_handle() {
    let ex = Executor::new();
    let child = ex.spawn(async { yield_now().await; 41 });
    let parent = ex.spawn(async move { child.await + 1 });
    ex.run();
    check!("the parent saw the child's output", parent.take(), Some(42));
}

#[test]
fn tasks_can_spawn_tasks() {
    let ex = Executor::new();
    let ex2 = ex.clone();
    let top = ex.spawn(async move {
        let a = ex2.spawn(async { 1 });
        let b = ex2.spawn(async { yield_now().await; 2 });
        a.await + b.await
    });
    ex.run();
    check!("sum of the spawned children", top.take(), Some(3));
}
