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
fn the_joiner_is_woken_after_the_future_is_destroyed() {
    struct Inspect(Arc<AtomicBool>, Mutex<Vec<bool>>);
    impl Wake for Inspect {
        fn wake(self: Arc<Self>) {
            let gone = self.0.load(Ordering::SeqCst);
            self.1.lock().unwrap().push(gone);
        }
    }
    // finished, panicked, aborted and thrown away
    for case in 0..4 {
        let (queue, sched) = new_queue();
        let (gone, guard) = flag();
        let keep: Arc<Mutex<Option<Waker>>> = Arc::default();
        let k = keep.clone();
        let mut handle = spawn_on(&sched, async move {
            let _guard = guard;
            match case {
                0 => 1,
                1 => panic!("x"),
                2 => {
                    poll_fn(move |cx| {
                        *k.lock().unwrap() = Some(cx.waker().clone());
                        Poll::Pending
                    })
                    .await
                }
                _ => std::future::pending().await,
            }
        });
        let inspect = Arc::new(Inspect(gone.clone(), Mutex::new(Vec::new())));
        let waker = Waker::from(inspect.clone());
        assert!(Pin::new(&mut handle).poll(&mut Context::from_waker(&waker)).is_pending());
        match case {
            0 | 1 => drop(queue.drain()),
            2 => {
                queue.step();
                handle.abort();
                queue.drain();
            }
            _ => drop(queue.pop()),
        }
        check!(format!("case {case}: woken once, with the future already gone"), inspect.1.lock().unwrap().clone(), vec![true]);
        assert!(handle.is_finished());
    }
}

#[test]
fn is_finished_is_false_while_the_future_lives() {
    struct Check(Arc<Mutex<Option<JoinHandle<()>>>>, Arc<Mutex<Vec<bool>>>);
    impl Drop for Check {
        fn drop(&mut self) {
            if let Some(h) = self.0.lock().unwrap().as_ref() {
                self.1.lock().unwrap().push(h.is_finished());
            }
        }
    }
    let (queue, sched) = new_queue();
    let slot = Arc::new(Mutex::new(None));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let check = Check(slot.clone(), seen.clone());
    let handle = spawn_on(&sched, async move {
        let _check = check;
    });
    *slot.lock().unwrap() = Some(handle);
    queue.drain();
    check!("asked from the destructor", seen.lock().unwrap().clone(), vec![false]);
    check!("afterwards", slot.lock().unwrap().as_ref().unwrap().is_finished(), true);
}

#[test]
fn a_destructor_panic_is_reported_and_contained() {
    struct Bomb;
    impl Drop for Bomb {
        fn drop(&mut self) {
            panic!("bomb");
        }
    }
    let (queue, sched) = new_queue();
    let mut done = spawn_on(&sched, async {
        let _bomb = Bomb;
        3
    });
    let escaped = catch_unwind(AssertUnwindSafe(|| queue.drain())).is_err();
    check!("run did not panic", escaped, false);
    check!("after finishing", peek(&mut done), Poll::Ready(Err(JoinError::Panicked("bomb".into()))));
    let keep: Arc<Mutex<Option<Waker>>> = Arc::default();
    let k = keep.clone();
    let mut stuck = spawn_on(&sched, async move {
        let _bomb = Bomb;
        poll_fn(move |cx| {
            *k.lock().unwrap() = Some(cx.waker().clone());
            Poll::<u8>::Pending
        })
        .await
    });
    queue.drain();
    stuck.abort();
    let escaped = catch_unwind(AssertUnwindSafe(|| queue.drain())).is_err();
    check!("cancelling does not panic either", escaped, false);
    check!("after abort", peek(&mut stuck), Poll::Ready(Err(JoinError::Panicked("bomb".into()))));
}

#[test]
fn an_aborted_task_is_not_polled_again() {
    let (queue, sched) = new_queue();
    let polls = Arc::new(AtomicUsize::new(0));
    let p = polls.clone();
    let mut handle = spawn_on(&sched, poll_fn(move |cx| {
        p.fetch_add(1, Ordering::SeqCst);
        cx.waker().wake_by_ref();
        Poll::<()>::Pending
    }));
    queue.step();
    queue.step();
    handle.abort();
    handle.abort();
    check!("already queued: not queued twice", queue.len(), 1);
    queue.drain();
    check!("no further polls", polls.load(Ordering::SeqCst), 2);
    check!("cancelled", peek(&mut handle), Poll::Ready(Err(JoinError::Cancelled)));
}

#[test]
fn abort_before_the_first_run() {
    let (queue, sched) = new_queue();
    let polls = Arc::new(AtomicUsize::new(0));
    let p = polls.clone();
    let mut handle = spawn_on(&sched, async move {
        p.fetch_add(1, Ordering::SeqCst);
    });
    handle.abort();
    check!("one entry", queue.len(), 1);
    queue.drain();
    check!("never polled", polls.load(Ordering::SeqCst), 0);
    check!("cancelled", peek(&mut handle), Poll::Ready(Err(JoinError::Cancelled)));
}

/// A task that aborts itself while it is being polled, then returns `Ready` or `Pending`.
#[test]
fn abort_during_a_poll() {
    for finishes in [true, false] {
        let (queue, sched) = new_queue();
        let slot: Arc<Mutex<Option<JoinHandle<u8>>>> = Arc::default();
        let polls = Arc::new(AtomicUsize::new(0));
        let (s, p) = (slot.clone(), polls.clone());
        let handle = spawn_on(&sched, poll_fn(move |_| {
            p.fetch_add(1, Ordering::SeqCst);
            s.lock().unwrap().as_ref().unwrap().abort();
            if finishes {
                Poll::Ready(9)
            } else {
                Poll::Pending
            }
        }));
        *slot.lock().unwrap() = Some(handle);
        queue.drain();
        let mut handle = slot.lock().unwrap().take().unwrap();
        let want = if finishes { Ok(9) } else { Err(JoinError::Cancelled) };
        check!(format!("poll finishes: {finishes}"), peek(&mut handle), Poll::Ready(want));
        check!("polled once", polls.load(Ordering::SeqCst), 1);
    }
}

#[test]
fn a_thrown_away_task_resolves_to_cancelled() {
    let (queue, sched) = new_queue();
    let (gone, guard) = flag();
    let mut handle = spawn_on(&sched, forever(guard));
    // never run: the scheduler just drops it
    drop(queue.pop());
    check!("destroyed", gone.load(Ordering::SeqCst), true);
    check!("cancelled", peek(&mut handle), Poll::Ready(Err(JoinError::Cancelled)));
    // run once, keeps no waker: the run itself lets go of the last reference
    let (gone, guard) = flag();
    let mut handle = spawn_on(&sched, forever(guard));
    queue.drain();
    check!("nobody can wake it, so it is destroyed", gone.load(Ordering::SeqCst), true);
    check!("cancelled", peek(&mut handle), Poll::Ready(Err(JoinError::Cancelled)));
    // aborting what is already gone does nothing
    handle.abort();
    check!("nothing queued", queue.len(), 0);
}

#[test]
fn the_latest_joiner_waker_is_woken() {
    let (queue, sched) = new_queue();
    let mut handle = spawn_on(&sched, async { 1 });
    let old = Arc::new(Count(AtomicUsize::new(0)));
    let new = Arc::new(Count(AtomicUsize::new(0)));
    for c in [&old, &new] {
        let waker = Waker::from(c.clone());
        assert!(Pin::new(&mut handle).poll(&mut Context::from_waker(&waker)).is_pending());
    }
    queue.drain();
    check!("(old, new)", (old.0.load(Ordering::SeqCst), new.0.load(Ordering::SeqCst)), (0, 1));
}

#[test]
fn polling_a_finished_handle_panics() {
    let (queue, sched) = new_queue();
    let mut handle = spawn_on(&sched, async { 1 });
    queue.drain();
    check!("first", peek(&mut handle), Poll::Ready(Ok(1)));
    let again = catch_unwind(AssertUnwindSafe(|| peek(&mut handle)));
    check!("second panics", again.is_err(), true);
}

#[test]
fn handles_are_send() {
    fn is_send<T: Send>() {}
    is_send::<JoinHandle<std::cell::Cell<u8>>>();
}

#[test]
fn random_aborts_and_runs_match_a_model() {
    let mut rng = anneal_prelude::Rng::new(120_201);
    for round in 0..300 {
        let (queue, sched) = new_queue();
        let n = 1 + rng.below(5);
        let waits: Vec<usize> = (0..n).map(|_| rng.below(4)).collect();
        let mut handles: Vec<JoinHandle<usize>> = (0..n)
            .map(|i| {
                let mut left = waits[i];
                spawn_on(&sched, poll_fn(move |cx| {
                    if left == 0 {
                        Poll::Ready(i)
                    } else {
                        left -= 1;
                        cx.waker().wake_by_ref();
                        Poll::Pending
                    }
                }))
            })
            .collect();
        // model: tasks are served first in, first out; each run uses up one wait
        let mut order: VecDeque<usize> = (0..n).collect();
        let mut left = waits.clone();
        let mut aborted = vec![false; n];
        let mut result: Vec<Option<Result<usize, JoinError>>> = vec![None; n];
        let mut history = format!("waits {waits:?}:");
        while let Some(i) = order.pop_front() {
            if rng.below(3) == 0 {
                let j = rng.below(n);
                history.push_str(&format!(" abort {j};"));
                handles[j].abort();
                aborted[j] = true;
            }
            history.push_str(&format!(" run {i};"));
            assert!(queue.step(), "round {round}: nothing queued, {history}");
            if aborted[i] {
                result[i] = Some(Err(JoinError::Cancelled));
            } else if left[i] == 0 {
                result[i] = Some(Ok(i));
            } else {
                left[i] -= 1;
                order.push_back(i);
            }
            for k in 0..n {
                check!(format!("round {round} {history} finished {k}?"), handles[k].is_finished(), result[k].is_some());
            }
        }
        check!(format!("round {round}: nothing left"), queue.len(), 0);
        for k in 0..n {
            let want = result[k].clone().expect("every task ended");
            check!(format!("round {round} {history} result {k}"), peek(&mut handles[k]), Poll::Ready(want));
        }
    }
}

#[test]
fn runners_and_aborters_on_many_threads() {
    for round in 0..20 {
        let (queue, sched) = new_queue();
        let alive = Arc::new(AtomicUsize::new(0));
        struct Alive(Arc<AtomicUsize>);
        impl Drop for Alive {
            fn drop(&mut self) {
                self.0.fetch_sub(1, Ordering::SeqCst);
            }
        }
        let handles: Vec<JoinHandle<u32>> = (0..64u32)
            .map(|i| {
                alive.fetch_add(1, Ordering::SeqCst);
                let guard = Alive(alive.clone());
                let mut left = i % 7;
                spawn_on(&sched, poll_fn(move |cx| {
                    let _ = &guard;
                    if left == 0 {
                        Poll::Ready(i)
                    } else {
                        left -= 1;
                        cx.waker().wake_by_ref();
                        Poll::Pending
                    }
                }))
            })
            .collect();
        let handles = Arc::new(handles);
        let runners: Vec<_> = (0..3)
            .map(|_| {
                let (queue, alive) = (queue.clone(), alive.clone());
                std::thread::spawn(move || {
                    while alive.load(Ordering::SeqCst) > 0 {
                        if !queue.step() {
                            std::thread::yield_now();
                        }
                    }
                })
            })
            .collect();
        let h = handles.clone();
        let aborter = std::thread::spawn(move || h.iter().step_by(3).for_each(|h| h.abort()));
        aborter.join().unwrap();
        runners.into_iter().for_each(|r| r.join().unwrap());
        queue.drain();
        let mut handles = Arc::into_inner(handles).unwrap();
        for (i, handle) in handles.iter_mut().enumerate() {
            let got = peek(handle);
            let fine = got == Poll::Ready(Ok(i as u32)) || (i % 3 == 0 && got == Poll::Ready(Err(JoinError::Cancelled)));
            assert!(fine, "round {round}, task {i}: {got:?}");
        }
    }
}
