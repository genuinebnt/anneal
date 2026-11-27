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
fn spawn_schedules_once_and_does_not_poll() {
    let queue = Arc::new(Queue::default());
    let probe = Arc::new(Probe::default());
    let task = spawn(&queue, probed(&probe));
    check!("scheduled", queue.len(), 1);
    check!("not polled", n(&probe.polls), 0);
    check!("not complete", task.is_complete(), false);
}

#[test]
fn a_wake_queues_it_again_once() {
    let queue = Arc::new(Queue::default());
    let probe = Arc::new(Probe::default());
    spawn(&queue, probed(&probe));
    assert!(queue.step());
    check!("polled once, not queued", (n(&probe.polls), queue.len()), (1, 0));
    waker_of(&probe).wake_by_ref();
    waker_of(&probe).wake_by_ref();
    waker_of(&probe).wake();
    check!("three wakes, one entry", queue.len(), 1);
}

#[test]
fn a_wake_before_the_first_run_adds_nothing() {
    let queue = Arc::new(Queue::default());
    let probe = Arc::new(Probe::default());
    let task = spawn(&queue, probed(&probe));
    use std::task::Wake;
    task.clone().wake();
    check!("still one entry", queue.len(), 1);
}

#[test]
fn a_wake_during_the_poll_is_not_lost() {
    let queue = Arc::new(Queue::default());
    let polls = Arc::new(AtomicUsize::new(0));
    let p = polls.clone();
    spawn(&queue, poll_fn(move |cx| {
        if p.fetch_add(1, Ordering::SeqCst) == 0 {
            cx.waker().wake_by_ref();
            cx.waker().wake_by_ref();
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    }));
    queue.step();
    check!("queued once more", queue.len(), 1);
    queue.step();
    check!("polled twice", n(&polls), 2);
}

#[test]
fn ready_drops_the_future_before_run_returns() {
    let queue = Arc::new(Queue::default());
    let probe = Arc::new(Probe::default());
    probe.finish.store(true, Ordering::SeqCst);
    let task = spawn(&queue, probed(&probe));
    queue.step();
    check!("complete and dropped", (task.is_complete(), probe.dropped.load(Ordering::SeqCst)), (true, true));
}

#[test]
fn wakes_after_completion_do_nothing() {
    let queue = Arc::new(Queue::default());
    let probe = Arc::new(Probe::default());
    probe.finish.store(true, Ordering::SeqCst);
    spawn(&queue, probed(&probe));
    queue.step();
    waker_of(&probe).wake();
    check!("nothing queued", queue.len(), 0);
}
