use solution::*;

#[allow(dead_code, unused_imports)]
use std::collections::VecDeque;
#[allow(dead_code, unused_imports)]
use std::future::{poll_fn, Future};
#[allow(dead_code, unused_imports)]
use std::panic::{catch_unwind, AssertUnwindSafe};
#[allow(dead_code, unused_imports)]
use std::pin::Pin;
#[allow(dead_code, unused_imports)]
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
#[allow(dead_code, unused_imports)]
use std::sync::{Arc, Mutex};
#[allow(dead_code, unused_imports)]
use std::task::{Context, Poll, Wake, Waker};

/// A scheduler that only collects. The test takes tasks out and runs them.
#[allow(dead_code)]
#[derive(Default)]
struct Queue(Mutex<VecDeque<Arc<Task>>>);
impl Schedule for Queue {
    fn schedule(&self, task: Arc<Task>) {
        self.0.lock().unwrap().push_back(task);
    }
}

#[allow(dead_code)]
impl Queue {
    fn len(&self) -> usize {
        self.0.lock().unwrap().len()
    }
    fn pop(&self) -> Option<Arc<Task>> {
        self.0.lock().unwrap().pop_front()
    }
    fn step(&self) -> bool {
        let task = self.pop();
        task.map(|t| t.run()).is_some()
    }
    fn drain(&self) -> usize {
        let mut runs = 0;
        while self.step() {
            runs += 1;
            assert!(runs < 100_000, "the tasks never settle");
        }
        runs
    }
}

#[allow(dead_code)]
fn new_queue() -> (Arc<Queue>, Arc<dyn Schedule>) {
    let queue = Arc::new(Queue::default());
    (queue.clone(), queue)
}

/// Polls a handle once.
#[allow(dead_code)]
fn peek<T>(handle: &mut JoinHandle<T>) -> Poll<Result<T, JoinError>> {
    Pin::new(handle).poll(&mut Context::from_waker(Waker::noop()))
}

/// Sets the flag when dropped.
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

/// Holds `guard` and never finishes. It keeps no waker.
#[allow(dead_code)]
async fn forever(guard: DropFlag) -> u32 {
    let _guard = guard;
    std::future::pending().await
}

/// Holds `guard` and never finishes. It leaves its waker in `keep`, so the task stays wakeable.
#[allow(dead_code)]
fn parked(guard: DropFlag, keep: &Arc<Mutex<Option<Waker>>>) -> impl Future<Output = u32> + Send + 'static {
    let keep = keep.clone();
    poll_fn(move |cx| {
        let _ = &guard;
        *keep.lock().unwrap() = Some(cx.waker().clone());
        Poll::Pending
    })
}

#[allow(dead_code)]
struct Count(AtomicUsize);
impl Wake for Count {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn the_output_arrives() {
    let (queue, sched) = new_queue();
    let mut handle = spawn_on(&sched, async { 21 * 2 });
    check!("not run yet", (queue.len(), handle.is_finished()), (1, false));
    queue.drain();
    check!("finished", handle.is_finished(), true);
    check!("the result", peek(&mut handle), Poll::Ready(Ok(42)));
}

#[test]
fn a_panic_becomes_an_error_and_run_survives() {
    let (queue, sched) = new_queue();
    let mut handle = spawn_on(&sched, async {
        if true {
            panic!("boom {}", 7);
        }
        0u32
    });
    queue.drain();
    check!("the message", peek(&mut handle), Poll::Ready(Err(JoinError::Panicked("boom 7".into()))));
}

#[test]
fn abort_cancels_at_the_next_run() {
    let (queue, sched) = new_queue();
    let (gone, guard) = flag();
    let keep = Arc::default();
    let mut handle = spawn_on(&sched, parked(guard, &keep));
    queue.drain();
    handle.abort();
    check!("not destroyed by abort itself", gone.load(Ordering::SeqCst), false);
    check!("but scheduled", queue.len(), 1);
    queue.drain();
    check!("destroyed by the run", gone.load(Ordering::SeqCst), true);
    check!("cancelled", peek(&mut handle), Poll::Ready(Err(JoinError::Cancelled)));
}

#[test]
fn abort_after_the_end_keeps_the_result() {
    let (queue, sched) = new_queue();
    let mut handle = spawn_on(&sched, async { 5 });
    queue.drain();
    handle.abort();
    queue.drain();
    check!("the result stays", peek(&mut handle), Poll::Ready(Ok(5)));
}

#[test]
fn dropping_the_handle_detaches() {
    let (queue, sched) = new_queue();
    let (gone, guard) = flag();
    drop(spawn_on(&sched, async move {
        let _guard = guard;
    }));
    check!("still alive and queued", (queue.len(), gone.load(Ordering::SeqCst)), (1, false));
    queue.drain();
    check!("it ran", gone.load(Ordering::SeqCst), true);
}

#[test]
fn a_task_awaits_another() {
    let (queue, sched) = new_queue();
    let child = spawn_on(&sched, async { 4 });
    let mut parent = spawn_on(&sched, async move { child.await.map(|v| v * 10) });
    queue.drain();
    check!("through the handle", peek(&mut parent), Poll::Ready(Ok(Ok(40))));
}
