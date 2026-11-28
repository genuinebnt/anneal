use solution::*;

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
use std::task::{Context, Poll, Waker};

#[allow(dead_code)]
type Log = Arc<Mutex<Vec<String>>>;

#[allow(dead_code)]
fn note(log: &Log, line: impl Into<String>) {
    log.lock().unwrap().push(line.into());
}

#[allow(dead_code)]
fn lines(log: &Log) -> Vec<String> {
    log.lock().unwrap().clone()
}

/// Turns a hang into a failure.
#[allow(dead_code)]
fn within<T: Send + 'static>(seconds: u64, f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(std::time::Duration::from_secs(seconds))
        .expect("the test hung or panicked")
}

/// A one-shot signal that any thread can set.
#[allow(dead_code)]
#[derive(Default)]
struct Latch {
    set: AtomicBool,
    waker: Mutex<Option<Waker>>,
}
#[allow(dead_code)]
impl Latch {
    fn open(&self) {
        self.set.store(true, Ordering::SeqCst);
        let waker = self.waker.lock().unwrap().take();
        if let Some(w) = waker {
            w.wake();
        }
    }
    fn poll(&self, cx: &mut Context<'_>) -> Poll<()> {
        *self.waker.lock().unwrap() = Some(cx.waker().clone());
        if self.set.load(Ordering::SeqCst) {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}

#[allow(dead_code)]
async fn opened(latch: Arc<Latch>) {
    poll_fn(|cx| latch.poll(cx)).await
}

#[allow(dead_code)]
struct DropFlag(Arc<AtomicBool>);
impl Drop for DropFlag {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

#[test]
fn the_root_is_polled_first() {
    let rt = LocalRuntime::new();
    let log = Log::default();
    let l = log.clone();
    rt.spawn(async move { note(&l, "task") });
    let l = log.clone();
    rt.block_on(async move { note(&l, "root") });
    check!("only the root ran", lines(&log), vec!["root".to_string()]);
    check!("the task is still there", rt.run_until_stalled(), 1);
}

#[test]
fn a_root_that_wakes_itself_does_not_starve_tasks() {
    let polls = within(5, || {
        let rt = LocalRuntime::new();
        let done = Arc::new(AtomicBool::new(false));
        let d = done.clone();
        rt.spawn(async move {
            for _ in 0..10 {
                yield_now().await;
            }
            d.store(true, Ordering::SeqCst);
        });
        let mut polls = 0;
        rt.block_on(async {
            while !done.load(Ordering::SeqCst) {
                polls += 1;
                assert!(polls < 10_000, "the task starves");
                yield_now().await;
            }
        });
        polls
    });
    assert!(polls <= 12, "{polls} root polls for 11 task runs: a task must run between two root polls");
}

#[test]
fn busy_tasks_do_not_starve_the_root() {
    let runs_after_wake = within(5, || {
        let rt = LocalRuntime::new();
        let stop = Arc::new(AtomicBool::new(false));
        let runs = Arc::new(AtomicUsize::new(0));
        let latch = Arc::new(Latch::default());
        let woken_at = Arc::new(AtomicUsize::new(usize::MAX));
        for _ in 0..5 {
            let (stop, runs, latch, woken_at) = (stop.clone(), runs.clone(), latch.clone(), woken_at.clone());
            rt.spawn(async move {
                while !stop.load(Ordering::SeqCst) {
                    let n = runs.fetch_add(1, Ordering::SeqCst) + 1;
                    assert!(n < 10_000, "the root starves");
                    if n == 20 {
                        woken_at.store(n, Ordering::SeqCst);
                        latch.open();
                    }
                    yield_now().await;
                }
            });
        }
        rt.block_on(opened(latch));
        let at_return = runs.load(Ordering::SeqCst);
        stop.store(true, Ordering::SeqCst);
        rt.run_until_stalled();
        at_return - woken_at.load(Ordering::SeqCst)
    });
    assert!(runs_after_wake <= 1, "{runs_after_wake} more tasks ran after the root was woken");
}

#[test]
fn at_least_one_task_runs_between_two_root_polls() {
    let rt = LocalRuntime::new();
    let log = Log::default();
    for id in ["a", "b", "c"] {
        let log = log.clone();
        rt.spawn(async move {
            note(&log, id);
        });
    }
    let mut left = 3;
    rt.block_on(poll_fn(|cx| {
        note(&log, "R");
        if left == 0 {
            return Poll::Ready(());
        }
        left -= 1;
        cx.waker().wake_by_ref();
        Poll::Pending
    }));
    check!("root and tasks alternate", lines(&log).concat(), "RaRbRcR".to_string());
}

#[test]
fn the_root_is_only_polled_when_woken() {
    let polls = within(5, || {
        let rt = LocalRuntime::new();
        let latch = Arc::new(Latch::default());
        let l = latch.clone();
        rt.spawn(async move {
            for _ in 0..50 {
                yield_now().await;
            }
            l.open();
        });
        let mut polls = 0;
        rt.block_on(poll_fn(|cx| {
            polls += 1;
            latch.poll(cx)
        }));
        polls
    });
    check!("once at the start, once after the wake", polls, 2);
}

#[test]
fn it_sleeps_while_idle() {
    let polls = within(5, || {
        let rt = LocalRuntime::new();
        let latch = Arc::new(Latch::default());
        let l = latch.clone();
        let t = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(60));
            l.open();
        });
        let mut polls = 0;
        rt.block_on(poll_fn(|cx| {
            polls += 1;
            latch.poll(cx)
        }));
        t.join().unwrap();
        polls
    });
    check!("not polled while waiting", polls, 2);
}

#[test]
fn other_threads_can_spawn_and_wake_tasks() {
    within(8, || {
        let rt = LocalRuntime::new();
        let handle = rt.handle();
        for _ in 0..200 {
            let latch = Arc::new(Latch::default());
            let done = Arc::new(Latch::default());
            let (l, d) = (latch.clone(), done.clone());
            // a task that waits for the other thread
            rt.spawn(async move {
                opened(l).await;
                d.open();
            });
            let (h, l) = (handle.clone(), latch.clone());
            let t = std::thread::spawn(move || {
                h.spawn(async move { l.open() });
            });
            rt.block_on(opened(done));
            t.join().unwrap();
        }
    });
}

#[test]
fn current_follows_the_runtime_that_is_running() {
    let a = LocalRuntime::new();
    let b = LocalRuntime::new();
    check!("outside", Handle::try_current().is_none(), true);
    let from_a = a.block_on(async {
        let [in_b] = b.block_on(async { [spawn(async { "b" })] });
        // back in a: this spawn goes to a
        let in_a = spawn(async { "a" });
        (in_b, in_a)
    });
    check!("outside again", Handle::try_current().is_none(), true);
    let (mut in_b, mut in_a) = from_a;
    check!("a's run does one task", a.run_until_stalled(), 1);
    check!("b's run does one task", b.run_until_stalled(), 1);
    let mut cx = Context::from_waker(Waker::noop());
    check!("a", Pin::new(&mut in_a).poll(&mut cx), Poll::Ready(Ok("a")));
    check!("b", Pin::new(&mut in_b).poll(&mut cx), Poll::Ready(Ok("b")));
}

#[test]
fn tasks_see_the_current_handle_under_run_until_stalled() {
    let rt = LocalRuntime::new();
    let mut h = rt.spawn(async { spawn(async { 1 }).await.unwrap() });
    rt.run_until_stalled();
    let mut cx = Context::from_waker(Waker::noop());
    check!("worked", Pin::new(&mut h).poll(&mut cx), Poll::Ready(Ok(1)));
}

#[test]
fn enter_guards_nest_in_any_order_and_survive_panics() {
    let a = LocalRuntime::new();
    let b = LocalRuntime::new();
    let ga = a.handle().enter();
    let gb = b.handle().enter();
    drop(ga);
    drop(spawn(async {}));
    check!("went to b", (a.run_until_stalled(), b.run_until_stalled()), (0, 1));
    drop(gb);
    check!("none", Handle::try_current().is_none(), true);
    let r = catch_unwind(AssertUnwindSafe(|| a.block_on(async { panic!("root") })));
    check!("the panic comes through", r.is_err(), true);
    check!("and current is restored", Handle::try_current().is_none(), true);
    check!("the runtime still works", a.block_on(async { spawn(async { 2 }).await }), Ok(2));
}

#[test]
fn current_is_per_thread() {
    let rt = LocalRuntime::new();
    let _g = rt.handle().enter();
    let other = std::thread::spawn(|| Handle::try_current().is_none()).join().unwrap();
    check!("a new thread has none", other, true);
}

#[test]
fn dropping_the_runtime_destroys_queued_tasks() {
    let rt = LocalRuntime::new();
    let gone: Vec<Arc<AtomicBool>> = (0..5).map(|_| Arc::default()).collect();
    let mut handles: Vec<_> = gone
        .iter()
        .map(|g| {
            let guard = DropFlag(g.clone());
            rt.spawn(async move {
                let _guard = guard;
            })
        })
        .collect();
    drop(rt);
    check!("all destroyed", gone.iter().all(|g| g.load(Ordering::SeqCst)), true);
    for h in &mut handles {
        check!("cancelled", Pin::new(h).poll(&mut Context::from_waker(Waker::noop())), Poll::Ready(Err(JoinError::Cancelled)));
    }
}

#[test]
fn destructors_may_spawn_while_the_runtime_is_dropped() {
    struct SpawnOnDrop(Arc<AtomicBool>, Arc<AtomicBool>);
    impl Drop for SpawnOnDrop {
        fn drop(&mut self) {
            self.1.store(Handle::try_current().is_some(), Ordering::SeqCst);
            let guard = DropFlag(self.0.clone());
            if let Some(h) = Handle::try_current() {
                drop(h.spawn(async move {
                    let _guard = guard;
                }));
            }
        }
    }
    let seen = within(5, || {
        let rt = LocalRuntime::new();
        let gone = Arc::new(AtomicBool::new(false));
        let current = Arc::new(AtomicBool::new(false));
        let probe = SpawnOnDrop(gone.clone(), current.clone());
        rt.spawn(async move {
            let _probe = probe;
        });
        drop(rt);
        (current.load(Ordering::SeqCst), gone.load(Ordering::SeqCst))
    });
    check!("(a runtime is current in the destructor, the task it spawned is destroyed too)", seen, (true, true));
    check!("current is restored after the drop", Handle::try_current().is_none(), true);
}

#[test]
fn spawn_after_drop_is_cancelled() {
    let handle = {
        let rt = LocalRuntime::new();
        rt.handle()
    };
    let gone = Arc::new(AtomicBool::new(false));
    let guard = DropFlag(gone.clone());
    let mut join = handle.spawn(async move {
        let _guard = guard;
    });
    check!("destroyed at once", gone.load(Ordering::SeqCst), true);
    check!("cancelled", Pin::new(&mut join).poll(&mut Context::from_waker(Waker::noop())), Poll::Ready(Err(JoinError::Cancelled)));
}

#[test]
fn yield_now_wakes_before_it_is_pending() {
    use std::task::Wake;
    struct Count(AtomicUsize);
    impl Wake for Count {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let count = Arc::new(Count(AtomicUsize::new(0)));
    let waker = Waker::from(count.clone());
    let mut f = Box::pin(yield_now());
    check!("pending", f.as_mut().poll(&mut Context::from_waker(&waker)), Poll::Pending);
    check!("woke itself", count.0.load(Ordering::SeqCst), 1);
}

#[test]
fn random_programs_run_in_fifo_order() {
    let mut rng = anneal_prelude::Rng::new(120_301);
    for round in 0..200 {
        let n = 1 + rng.below(6);
        let yields: Vec<usize> = (0..n).map(|_| rng.below(5)).collect();
        let rt = LocalRuntime::new();
        let log = Log::default();
        for (id, &y) in yields.iter().enumerate() {
            let log = log.clone();
            rt.spawn(async move {
                for _ in 0..y {
                    note(&log, id.to_string());
                    yield_now().await;
                }
                note(&log, id.to_string());
            });
        }
        let runs = rt.run_until_stalled();
        let mut queue: std::collections::VecDeque<(usize, usize)> = yields.iter().copied().enumerate().collect();
        let mut want = Vec::new();
        while let Some((id, left)) = queue.pop_front() {
            want.push(id.to_string());
            if left > 0 {
                queue.push_back((id, left - 1));
            }
        }
        check!(format!("round {round}, yields {yields:?}: number of runs"), runs, want.len());
        check!(format!("round {round}, yields {yields:?}: order"), lines(&log), want);
    }
}

#[test]
fn many_tasks_and_a_deep_chain() {
    let rt = LocalRuntime::new();
    let total = rt.block_on(async {
        let mut prev = spawn(async { 0u32 });
        for _ in 0..2000 {
            prev = spawn(async move { prev.await.unwrap() + 1 });
        }
        prev.await.unwrap()
    });
    check!("2000 tasks each waiting for the one before", total, 2000);
}

#[test]
fn a_queued_task_is_destroyed_even_if_someone_holds_its_waker() {
    // the task waits on two latches. One opens, so the task is queued, while the other still holds a waker for it
    let rt = LocalRuntime::new();
    let (never, opens) = (Arc::new(Latch::default()), Arc::new(Latch::default()));
    let gone = Arc::new(AtomicBool::new(false));
    let guard = DropFlag(gone.clone());
    let (a, b) = (never.clone(), opens.clone());
    let mut join = rt.spawn(async move {
        let _guard = guard;
        poll_fn(|cx| {
            let _ = a.poll(cx);
            let _ = b.poll(cx);
            Poll::<()>::Pending
        })
        .await
    });
    check!("runs", rt.run_until_stalled(), 1);
    opens.open();
    drop(rt);
    check!("a queued task is destroyed, although a waker for it is still held", gone.load(Ordering::SeqCst), true);
    check!("cancelled", Pin::new(&mut join).poll(&mut Context::from_waker(Waker::noop())), Poll::Ready(Err(JoinError::Cancelled)));
    drop(never);
}
