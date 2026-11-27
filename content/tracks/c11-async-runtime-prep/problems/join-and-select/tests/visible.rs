use solution::*;

#[allow(dead_code, unused_imports)]
use std::cell::RefCell;
#[allow(dead_code, unused_imports)]
use std::future::Future;
#[allow(dead_code, unused_imports)]
use std::pin::Pin;
#[allow(dead_code, unused_imports)]
use std::rc::Rc;
#[allow(dead_code, unused_imports)]
use std::task::{Context, Poll, Waker};

#[allow(dead_code)]
type Log = Rc<RefCell<Vec<String>>>;

/// Pending `waits` times, then ready with `id`. It writes down every poll and its drop.
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

#[test]
fn join_all_returns_outputs_in_input_order() {
    let log = Log::default();
    let mut all = Box::pin(join_all(vec![script(&log, 10, 2), script(&log, 11, 0), script(&log, 12, 1)]));
    check!("first poll", step(all.as_mut()), Poll::Pending);
    check!("second poll", step(all.as_mut()), Poll::Pending);
    check!("third poll", step(all.as_mut()), Poll::Ready(vec![10, 11, 12]));
}

#[test]
fn every_unfinished_child_is_polled_each_time() {
    let log = Log::default();
    let mut all = Box::pin(join_all(vec![script(&log, 1, 2), script(&log, 2, 1), script(&log, 3, 2)]));
    let _ = step(all.as_mut());
    check!("poll 1 reaches all three", take(&log), vec!["poll 1", "poll 2", "poll 3"]);
    let _ = step(all.as_mut());
    check!("poll 2: child 2 finishes and is dropped at once", take(&log), vec!["poll 1", "poll 2", "drop 2", "poll 3"]);
    let _ = step(all.as_mut());
    check!("poll 3 leaves child 2 alone", take(&log), vec!["poll 1", "drop 1", "poll 3", "drop 3"]);
}

#[test]
fn join_all_of_nothing_is_ready() {
    let mut all = Box::pin(join_all(Vec::<Script>::new()));
    check!("no children", step(all.as_mut()), Poll::Ready(Vec::<u32>::new()));
}

#[test]
fn select_prefers_the_left_side() {
    let log = Log::default();
    let mut first = Box::pin(select(script(&log, 1, 0), script(&log, 2, 0)));
    check!("both are ready at once", step(first.as_mut()), Poll::Ready(Either::Left(1)));
    let seen = take(&log);
    check!("the right side was never polled", seen.contains(&"poll 2".to_string()), false);
}

#[test]
fn select_drops_the_loser_before_it_returns() {
    let log = Log::default();
    let mut race = Box::pin(select(script(&log, 1, 5), script(&log, 2, 1)));
    check!("neither is ready", step(race.as_mut()), Poll::Pending);
    check!("the right side wins", step(race.as_mut()), Poll::Ready(Either::Right(2)));
    let mut drops: Vec<String> = take(&log).into_iter().filter(|l| l.starts_with("drop")).collect();
    drops.sort();
    check!("both children are gone while the select is still alive", drops, vec!["drop 1", "drop 2"]);
}

#[test]
fn async_blocks_work_as_children() {
    let text = String::from("borrowed");
    let mut all = Box::pin(join_all(vec![&text, &text].into_iter().map(|t| async move { t.len() }).collect()));
    check!("children that are not Unpin and borrow a local", step(all.as_mut()), Poll::Ready(vec![8, 8]));
}
