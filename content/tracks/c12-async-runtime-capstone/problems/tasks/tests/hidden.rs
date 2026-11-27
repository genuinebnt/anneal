use solution::*;

#[allow(dead_code, unused_imports)]
use std::collections::VecDeque;
#[allow(dead_code, unused_imports)]
use std::future::{poll_fn, Future};
#[allow(dead_code, unused_imports)]
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
#[allow(dead_code, unused_imports)]
use std::sync::{Arc, Mutex};
#[allow(dead_code, unused_imports)]
use std::task::{Context, Poll, Waker};

/// A scheduler that only collects. The test takes tasks out and runs them.
#[allow(dead_code)]
#[derive(Default)]
struct Queue {
    tasks: Mutex<VecDeque<Arc<Task>>>,
    scheduled: AtomicUsize,
}
impl Schedule for Queue {
    fn schedule(&self, task: Arc<Task>) {
        self.scheduled.fetch_add(1, Ordering::SeqCst);
        self.tasks.lock().unwrap().push_back(task);
    }
}

#[allow(dead_code)]
impl Queue {
    fn len(&self) -> usize {
        self.tasks.lock().unwrap().len()
    }
    fn pop(&self) -> Option<Arc<Task>> {
        self.tasks.lock().unwrap().pop_front()
    }
    /// Runs the oldest waiting task; `false` if there is none.
    fn step(&self) -> bool {
        let task = self.pop();
        task.map(|t| t.run()).is_some()
    }
}

/// What a test can see of its future.
#[allow(dead_code)]
#[derive(Default)]
struct Probe {
    polls: AtomicUsize,
    finish: AtomicBool,
    dropped: AtomicBool,
    polling: AtomicBool,
    waker: Mutex<Option<Waker>>,
}

#[allow(dead_code)]
struct DropFlag(Arc<Probe>);
impl Drop for DropFlag {
    fn drop(&mut self) {
        self.0.dropped.store(true, Ordering::SeqCst);
    }
}

/// Pending until `finish` is set; stores its waker on every poll.
#[allow(dead_code)]
fn probed(probe: &Arc<Probe>) -> impl Future<Output = ()> + Send + 'static {
    let (p, flag) = (probe.clone(), DropFlag(probe.clone()));
    poll_fn(move |cx| {
        let _ = &flag;
        p.polls.fetch_add(1, Ordering::SeqCst);
        *p.waker.lock().unwrap() = Some(cx.waker().clone());
        if p.finish.load(Ordering::SeqCst) {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    })
}

#[allow(dead_code)]
fn spawn(queue: &Arc<Queue>, future: impl Future<Output = ()> + Send + 'static) -> Arc<Task> {
    Task::spawn(future, queue.clone())
}

#[allow(dead_code)]
fn waker_of(probe: &Arc<Probe>) -> Waker {
    probe.waker.lock().unwrap().clone().expect("the future has been polled")
}

#[allow(dead_code)]
fn n(counter: &AtomicUsize) -> usize {
    counter.load(Ordering::SeqCst)
}

#[test]
fn schedule_is_never_called_during_a_poll() {
    // the scheduler checks whether the future is being polled at the moment it is handed the task
    #[derive(Default)]
    struct Strict {
        queue: Queue,
        probe: Arc<Probe>,
        violations: AtomicUsize,
    }
    impl Schedule for Strict {
        fn schedule(&self, task: Arc<Task>) {
            if self.probe.polling.load(Ordering::SeqCst) {
                self.violations.fetch_add(1, Ordering::SeqCst);
            }
            self.queue.schedule(task);
        }
    }
    let strict = Arc::new(Strict::default());
    let p = strict.probe.clone();
    Task::spawn(
        poll_fn(move |cx| {
            p.polling.store(true, Ordering::SeqCst);
            let waker = cx.waker().clone();
            // woken here, and from another thread, while the poll is in progress
            waker.wake_by_ref();
            std::thread::spawn(move || waker.wake()).join().unwrap();
            p.polling.store(false, Ordering::SeqCst);
            if p.polls.fetch_add(1, Ordering::SeqCst) < 3 {
                Poll::Pending
            } else {
                Poll::Ready(())
            }
        }),
        strict.clone(),
    );
    while strict.queue.step() {}
    check!("(violations, polls, entries made)", (n(&strict.violations), n(&strict.probe.polls), n(&strict.queue.scheduled)), (0, 4, 4));
}

#[test]
fn schedule_may_run_the_task_at_once() {
    struct Inline;
    impl Schedule for Inline {
        fn schedule(&self, task: Arc<Task>) {
            task.run();
        }
    }
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut left = 50;
        let task = Task::spawn(
            poll_fn(move |cx| {
                if left == 0 {
                    return Poll::Ready(());
                }
                left -= 1;
                cx.waker().wake_by_ref();
                Poll::Pending
            }),
            Arc::new(Inline),
        );
        tx.send(task.is_complete()).unwrap();
    });
    let done = rx.recv_timeout(std::time::Duration::from_secs(5)).expect("deadlock or panic when schedule runs the task");
    check!("ran to completion inside spawn", done, true);
}

#[test]
fn a_destructor_that_wakes_its_own_task() {
    struct WakeOnDrop(Option<Waker>);
    impl Drop for WakeOnDrop {
        fn drop(&mut self) {
            if let Some(w) = self.0.take() {
                w.wake();
            }
        }
    }
    let queue = Arc::new(Queue::default());
    let mut guard = WakeOnDrop(None);
    spawn(&queue, poll_fn(move |cx| {
        guard.0 = Some(cx.waker().clone());
        Poll::Ready(())
    }));
    queue.step();
    check!("nothing queued", queue.len(), 0);
}

#[test]
fn a_finished_future_is_dropped_while_wakers_live_on() {
    let queue = Arc::new(Queue::default());
    let probe = Arc::new(Probe::default());
    spawn(&queue, probed(&probe));
    queue.step();
    let kept = waker_of(&probe);
    probe.finish.store(true, Ordering::SeqCst);
    kept.wake_by_ref();
    queue.step();
    check!("dropped although a waker is alive", probe.dropped.load(Ordering::SeqCst), true);
    kept.wake();
    check!("nothing queued", queue.len(), 0);
}

#[test]
fn everything_is_freed_when_the_last_waker_goes() {
    let queue = Arc::new(Queue::default());
    let probe = Arc::new(Probe::default());
    let task = spawn(&queue, probed(&probe));
    queue.step();
    let weak = Arc::downgrade(&task);
    drop(task);
    let waker = probe.waker.lock().unwrap().take().unwrap();
    check!("a waker keeps the task", weak.upgrade().is_some(), true);
    drop(waker);
    check!("the task is gone", weak.upgrade().is_none(), true);
    check!("with its unfinished future", probe.dropped.load(Ordering::SeqCst), true);
    check!("and its hold on the scheduler", Arc::strong_count(&queue), 1);
}

#[test]
fn a_pending_poll_without_a_wake_is_not_requeued() {
    let queue = Arc::new(Queue::default());
    let probe = Arc::new(Probe::default());
    spawn(&queue, probed(&probe));
    queue.step();
    check!("idle", (queue.len(), n(&queue.scheduled)), (0, 1));
}

#[test]
fn random_wakes_and_runs_match_a_model() {
    let mut rng = anneal_prelude::Rng::new(120_101);
    for round in 0..300 {
        let queue = Arc::new(Queue::default());
        let probes: Vec<Arc<Probe>> = (0..4).map(|_| Arc::new(Probe::default())).collect();
        let tasks: Vec<Arc<Task>> = probes.iter().map(|p| spawn(&queue, probed(p))).collect();
        // model: which tasks are waiting for a run, in order, and which are complete
        let mut waiting: VecDeque<usize> = (0..4).collect();
        let mut complete = [false; 4];
        let mut history = Vec::new();
        for _ in 0..60 {
            match rng.below(4) {
                0 => {
                    if let Some(i) = waiting.pop_front() {
                        history.push(format!("run {i}"));
                        queue.step();
                        complete[i] = probes[i].finish.load(Ordering::SeqCst);
                    }
                }
                1 | 2 => {
                    let i = rng.below(4);
                    let stored = probes[i].waker.lock().unwrap().clone();
                    if let Some(w) = stored {
                        history.push(format!("wake {i}"));
                        w.wake();
                        if !complete[i] && !waiting.contains(&i) {
                            waiting.push_back(i);
                        }
                    }
                }
                _ => {
                    let i = rng.below(4);
                    history.push(format!("allow {i} to finish"));
                    probes[i].finish.store(true, Ordering::SeqCst);
                }
            }
            check!(format!("round {round} {history:?}: queue length"), queue.len(), waiting.len());
            for i in 0..4 {
                check!(format!("round {round} {history:?}: task {i} complete"), tasks[i].is_complete(), complete[i]);
            }
        }
    }
}

/// Four threads run tasks while four others wake them. No task may be queued twice or polled twice at once, and the
/// last poll of each task must see everything written before the last wake.
#[test]
fn hammered_by_many_threads() {
    const TASKS: usize = 8;
    const WAKES: usize = 4000;
    struct Cell {
        probe: Arc<Probe>,
        written: AtomicUsize,
        seen: AtomicUsize,
        queued: AtomicBool,
    }
    struct Checked {
        queue: Queue,
        ids: Mutex<Vec<(usize, usize)>>,
        cells: Vec<Cell>,
        double: AtomicUsize,
    }
    impl Schedule for Checked {
        fn schedule(&self, task: Arc<Task>) {
            let key = Arc::as_ptr(&task) as usize;
            let id = self.ids.lock().unwrap().iter().find(|(k, _)| *k == key).map(|p| p.1);
            if let Some(id) = id {
                if self.cells[id].queued.swap(true, Ordering::SeqCst) {
                    self.double.fetch_add(1, Ordering::SeqCst);
                }
            }
            self.queue.schedule(task);
        }
    }
    for _ in 0..3 {
        let shared = Arc::new(Checked {
            queue: Queue::default(),
            ids: Mutex::new(Vec::new()),
            cells: (0..TASKS)
                .map(|_| Cell { probe: Arc::default(), written: AtomicUsize::new(0), seen: AtomicUsize::new(0), queued: AtomicBool::new(false) })
                .collect(),
            double: AtomicUsize::new(0),
        });
        let overlaps = Arc::new(AtomicUsize::new(0));
        for id in 0..TASKS {
            let (s, o) = (shared.clone(), overlaps.clone());
            let task = Task::spawn(
                poll_fn(move |cx| {
                    let cell = &s.cells[id];
                    if cell.probe.polling.swap(true, Ordering::SeqCst) {
                        o.fetch_add(1, Ordering::SeqCst);
                    }
                    *cell.probe.waker.lock().unwrap() = Some(cx.waker().clone());
                    cell.seen.store(cell.written.load(Ordering::Relaxed), Ordering::SeqCst);
                    std::hint::spin_loop();
                    cell.probe.polling.store(false, Ordering::SeqCst);
                    Poll::Pending
                }),
                shared.clone(),
            );
            // the first schedule happened before the id was known
            shared.ids.lock().unwrap().push((Arc::as_ptr(&task) as usize, id));
            shared.cells[id].queued.store(true, Ordering::SeqCst);
        }
        let wakers_done = Arc::new(AtomicUsize::new(0));
        let runners: Vec<_> = (0..3)
            .map(|_| {
                let (s, done) = (shared.clone(), wakers_done.clone());
                std::thread::spawn(move || loop {
                    let finished = done.load(Ordering::SeqCst) == 3;
                    match s.queue.pop() {
                        Some(task) => {
                            let key = Arc::as_ptr(&task) as usize;
                            let id = s.ids.lock().unwrap().iter().find(|(k, _)| *k == key).unwrap().1;
                            s.cells[id].queued.store(false, Ordering::SeqCst);
                            task.run();
                        }
                        None if finished => break,
                        None => std::thread::yield_now(),
                    }
                })
            })
            .collect();
        let wakers: Vec<_> = (0..3)
            .map(|t| {
                let (s, done) = (shared.clone(), wakers_done.clone());
                std::thread::spawn(move || {
                    let mut rng = anneal_prelude::Rng::new(120_102 + t);
                    for _ in 0..WAKES {
                        let cell = &s.cells[rng.below(TASKS)];
                        cell.written.fetch_add(1, Ordering::Relaxed);
                        let waker = cell.probe.waker.lock().unwrap().clone();
                        if let Some(w) = waker {
                            w.wake();
                        }
                    }
                    done.fetch_add(1, Ordering::SeqCst);
                })
            })
            .collect();
        wakers.into_iter().for_each(|t| t.join().unwrap());
        runners.into_iter().for_each(|t| t.join().unwrap());
        while shared.queue.step() {}
        check!("tasks queued twice", n(&shared.double), 0);
        check!("tasks polled by two threads at once", n(&overlaps), 0);
        for (id, cell) in shared.cells.iter().enumerate() {
            // every task has been polled at least once, so every write was followed by a wake
            check!(format!("task {id}: writes seen by its last poll"), n(&cell.seen), n(&cell.written));
        }
        for cell in &shared.cells {
            cell.probe.waker.lock().unwrap().take();
        }
    }
}

#[test]
fn tasks_are_send_and_sync() {
    fn both<T: Send + Sync>() {}
    both::<Task>();
}
