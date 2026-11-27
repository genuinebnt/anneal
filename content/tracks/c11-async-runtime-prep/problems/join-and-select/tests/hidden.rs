use solution::*;

#[allow(dead_code, unused_imports)]
use std::cell::{Cell, RefCell};
#[allow(dead_code, unused_imports)]
use std::future::Future;
#[allow(dead_code, unused_imports)]
use std::panic::{catch_unwind, AssertUnwindSafe};
#[allow(dead_code, unused_imports)]
use std::pin::Pin;
#[allow(dead_code, unused_imports)]
use std::rc::Rc;
#[allow(dead_code, unused_imports)]
use std::sync::atomic::{AtomicUsize, Ordering};
#[allow(dead_code, unused_imports)]
use std::sync::Arc;
#[allow(dead_code, unused_imports)]
use std::task::{Context, Poll, Wake, Waker};

#[allow(dead_code)]
type Log = Rc<RefCell<Vec<String>>>;

#[allow(dead_code)]
struct Script {
    id: u32,
    waits: u32,
    done: bool,
    log: Log,
}

#[allow(dead_code)]
fn script(log: &Log, id: u32, waits: u32) -> Script {
    Script { id, waits, done: false, log: log.clone() }
}

impl Future for Script {
    type Output = u32;
    fn poll(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<u32> {
        assert!(!self.done, "child {} was polled again after it finished", self.id);
        self.log.borrow_mut().push(format!("poll {}", self.id));
        if self.waits == 0 {
            self.done = true;
            Poll::Ready(self.id)
        } else {
            self.waits -= 1;
            Poll::Pending
        }
    }
}

impl Drop for Script {
    fn drop(&mut self) {
        self.log.borrow_mut().push(format!("drop {}", self.id));
    }
}

#[allow(dead_code)]
fn step<F: Future>(f: Pin<&mut F>) -> Poll<F::Output> {
    f.poll(&mut Context::from_waker(Waker::noop()))
}

#[allow(dead_code)]
fn take(log: &Log) -> Vec<String> {
    log.borrow_mut().drain(..).collect()
}

#[allow(dead_code)]
struct Counter(AtomicUsize);
impl Wake for Counter {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn children_get_the_callers_waker() {
    let counter = Arc::new(Counter(AtomicUsize::new(0)));
    let waker = Waker::from(counter.clone());
    let child = || std::future::poll_fn(|cx: &mut Context<'_>| {
        cx.waker().wake_by_ref();
        Poll::<()>::Pending
    });
    let mut all = Box::pin(join_all(vec![child(), child(), child()]));
    let _ = all.as_mut().poll(&mut Context::from_waker(&waker));
    check!("each child woke the caller's waker", counter.0.load(Ordering::SeqCst), 3);
    let mut race = Box::pin(select(child(), child()));
    let _ = race.as_mut().poll(&mut Context::from_waker(&waker));
    check!("both sides woke the caller's waker", counter.0.load(Ordering::SeqCst), 5);
}

#[test]
fn polling_a_finished_join_all_panics() {
    let log = Log::default();
    let mut all = Box::pin(join_all(vec![script(&log, 1, 0)]));
    check!("finishes", step(all.as_mut()), Poll::Ready(vec![1]));
    let again = catch_unwind(AssertUnwindSafe(|| step(all.as_mut())));
    check!("a second poll panics", again.is_err(), true);
}

#[test]
fn polling_a_finished_select_panics() {
    let log = Log::default();
    let mut race = Box::pin(select(script(&log, 1, 0), script(&log, 2, 9)));
    check!("finishes", step(race.as_mut()), Poll::Ready(Either::Left(1)));
    let again = catch_unwind(AssertUnwindSafe(|| step(race.as_mut())));
    check!("a second poll panics", again.is_err(), true);
}

#[test]
fn polling_an_empty_join_all_twice_panics_the_second_time() {
    let mut all = Box::pin(join_all(Vec::<Script>::new()));
    check!("ready once", step(all.as_mut()), Poll::Ready(Vec::<u32>::new()));
    let again = catch_unwind(AssertUnwindSafe(|| step(all.as_mut())));
    check!("a second poll panics", again.is_err(), true);
}

#[test]
fn dropping_join_all_drops_the_children_and_the_kept_outputs() {
    struct Tracked(Rc<Cell<u32>>);
    impl Drop for Tracked {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let outputs = Rc::new(Cell::new(0));
    let guards = Rc::new(Cell::new(0));
    let mut children: Vec<Pin<Box<dyn Future<Output = Tracked>>>> = Vec::new();
    for _ in 0..3 {
        let o = outputs.clone();
        children.push(Box::pin(async move { Tracked(o) }));
    }
    for _ in 0..4 {
        let g = guards.clone();
        children.push(Box::pin(async move {
            let _held = Tracked(g);
            std::future::pending::<Tracked>().await
        }));
    }
    let mut all = Box::pin(join_all(children));
    assert!(step(all.as_mut()).is_pending());
    check!("nothing dropped while the join_all lives", (outputs.get(), guards.get()), (0, 0));
    drop(all);
    check!("three kept outputs and four running children", (outputs.get(), guards.get()), (3, 4));
}

#[test]
fn dropping_select_drops_both_sides() {
    let log = Log::default();
    let mut race = Box::pin(select(script(&log, 1, 5), script(&log, 2, 5)));
    let _ = step(race.as_mut());
    take(&log);
    drop(race);
    let mut gone = take(&log);
    gone.sort();
    check!("both dropped", gone, vec!["drop 1", "drop 2"]);
}

#[test]
fn the_loser_is_dropped_when_the_left_side_wins() {
    let log = Log::default();
    let mut race = Box::pin(select(script(&log, 1, 1), script(&log, 2, 7)));
    let _ = step(race.as_mut());
    take(&log);
    check!("left wins", step(race.as_mut()), Poll::Ready(Either::Left(1)));
    let mut seen = take(&log);
    seen.sort();
    check!("the right side was not polled again, and both are gone", seen, vec!["drop 1", "drop 2", "poll 1"]);
}

#[test]
fn select_sides_may_have_different_types() {
    let mut race = Box::pin(select(std::future::pending::<u8>(), async { String::from("text") }));
    check!("right wins with its own type", step(race.as_mut()), Poll::Ready(Either::<u8, String>::Right("text".into())));
}

#[test]
fn nested_combinators() {
    let log = Log::default();
    let inner = |a, b| join_all(vec![script(&log, a, 1), script(&log, b, 2)]);
    let mut all = Box::pin(join_all(vec![inner(1, 2), inner(3, 4)]));
    assert!(step(all.as_mut()).is_pending());
    assert!(step(all.as_mut()).is_pending());
    check!("third poll", step(all.as_mut()), Poll::Ready(vec![vec![1, 2], vec![3, 4]]));
}

#[test]
fn random_scripts_against_a_model() {
    let mut rng = anneal_prelude::Rng::new(110_101);
    for _ in 0..300 {
        let n = rng.below(7);
        let waits: Vec<u32> = (0..n).map(|_| rng.below(5) as u32).collect();
        let log = Log::default();
        let children: Vec<Script> = waits.iter().enumerate().map(|(i, &w)| script(&log, i as u32, w)).collect();
        let mut all = Box::pin(join_all(children));
        let rounds = waits.iter().copied().max().unwrap_or(0);
        for round in 0..=rounds {
            let got = step(all.as_mut());
            let mut want = Vec::new();
            for (i, &w) in waits.iter().enumerate() {
                if w >= round {
                    want.push(format!("poll {i}"));
                    if w == round {
                        want.push(format!("drop {i}"));
                    }
                }
            }
            check!(format!("waits {waits:?}, round {round}: polls and drops"), take(&log), want);
            let expected = if round == rounds { Poll::Ready((0..n as u32).collect()) } else { Poll::Pending };
            check!(format!("waits {waits:?}, round {round}: result"), got, expected);
        }
    }
}

#[test]
fn random_races() {
    let mut rng = anneal_prelude::Rng::new(110_102);
    for _ in 0..200 {
        let (a, b) = (rng.below(5) as u32, rng.below(5) as u32);
        let log = Log::default();
        let mut race = Box::pin(select(script(&log, 1, a), script(&log, 2, b)));
        let mut polls = 0;
        let won = loop {
            polls += 1;
            if let Poll::Ready(v) = step(race.as_mut()) {
                break v;
            }
        };
        let want = if a <= b { Either::Left(1) } else { Either::Right(2) };
        check!(format!("a waits {a}, b waits {b}"), (won, polls), (want, a.min(b) + 1));
        let right_polls = take(&log).iter().filter(|l| *l == "poll 2").count() as u32;
        let expected = if a <= b { a } else { b + 1 };
        check!(format!("a waits {a}, b waits {b}: polls of the right side"), right_polls, expected);
    }
}
