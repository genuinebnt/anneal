use solution::*;


use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::mpsc::Receiver;

/// What is waiting in the queue now, and whether the hub has dropped its end.
#[allow(dead_code)]
fn drain<M>(queue: &mut Receiver<M>) -> (Vec<M>, bool) {
    let mut out = Vec::new();
    loop {
        match queue.try_recv() {
            Ok(m) => out.push(m),
            Err(TryRecvError::Empty) => return (out, false),
            Err(TryRecvError::Disconnected) => return (out, true),
        }
    }
}
#[allow(dead_code)]
fn report(delivered: usize, evicted: &[MemberId]) -> Delivery {
    Delivery { delivered, evicted: evicted.to_vec() }
}

#[test]
fn everyone_on_the_topic_gets_it() {
    let mut hub = Hub::new();
    let (a, mut qa) = hub.join(8);
    let (b, mut qb) = hub.join(8);
    let (_, mut qc) = hub.join(8);
    hub.subscribe(a, "news");
    hub.subscribe(b, "news");
    check!("two", hub.publish("news", "hi"), report(2, &[]));
    check!("a", drain(&mut qa), (vec!["hi"], false));
    check!("b", drain(&mut qb), (vec!["hi"], false));
    check!("c", drain(&mut qc), (vec![], false));
}

#[test]
fn a_reader_that_falls_behind_is_removed() {
    let mut hub = Hub::new();
    let (slow, mut q) = hub.join(2);
    hub.subscribe(slow, "t");
    hub.publish("t", 1);
    hub.publish("t", 2);
    check!("the third does not fit", hub.publish("t", 3), report(0, &[slow]));
    check!("what was queued can still be read, then the end", drain(&mut q), (vec![1, 2], true));
    check!("gone", (hub.members(), hub.publish("t", 4)), (0, report(0, &[])));
}

#[test]
fn subscribing_counts_topics() {
    let mut hub = Hub::<u8>::new();
    let (a, _q) = hub.join(1);
    check!("one", hub.subscribe(a, "x"), Some(1));
    check!("two", hub.subscribe(a, "y"), Some(2));
    check!("again", hub.subscribe(a, "x"), Some(2));
    check!("unsubscribe", hub.unsubscribe(a, "x"), Some(1));
    check!("a topic it is not on", hub.unsubscribe(a, "z"), Some(1));
    check!("a stranger", hub.subscribe(99, "x"), None);
}

#[test]
fn leaving() {
    let mut hub = Hub::new();
    let (a, mut q) = hub.join(4);
    hub.subscribe(a, "x");
    check!("leave", hub.leave(a), true);
    check!("twice", hub.leave(a), false);
    check!("no topics left", (hub.topics(), hub.members()), (0, 0));
    check!("publish reaches nobody", hub.publish("x", 'm'), report(0, &[]));
    check!("the queue has ended", drain(&mut q), (vec![], true));
}

#[test]
fn a_reader_that_went_away_is_not_counted() {
    let mut hub = Hub::new();
    let (a, q) = hub.join(4);
    hub.subscribe(a, "x");
    drop(q);
    check!("publish", hub.publish("x", 1), report(0, &[]));
    check!("and it is forgotten", (hub.members(), hub.topics()), (0, 0));
}
