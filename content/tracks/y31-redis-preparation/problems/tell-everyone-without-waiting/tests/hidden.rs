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
fn ids_are_never_reused() {
    let mut hub = Hub::<u8>::new();
    let (a, _qa) = hub.join(1);
    hub.leave(a);
    let (b, _qb) = hub.join(1);
    check!("new id", a != b, true);
}

#[test]
fn eviction_takes_the_member_off_every_topic() {
    let mut hub = Hub::new();
    let (a, mut q) = hub.join(1);
    hub.subscribe(a, "x");
    hub.subscribe(a, "y");
    let (b, mut qb) = hub.join(10);
    hub.subscribe(b, "y");
    hub.publish("x", 1);
    check!("full", hub.publish("x", 2), report(0, &[a]));
    check!("y still has b only", hub.publish("y", 3), report(1, &[]));
    check!("topics", hub.topics(), 1);
    check!("a is unknown now", hub.subscribe(a, "x"), None);
    check!("a read the first", drain(&mut q), (vec![1], true));
    check!("b", drain(&mut qb), (vec![3], false));
}

#[test]
fn one_slow_reader_does_not_hold_back_the_others() {
    let mut hub = Hub::new();
    let mut queues: Vec<(MemberId, Receiver<u32>)> = (0..5).map(|_| hub.join(3)).collect();
    for (id, _) in &queues {
        hub.subscribe(*id, "t");
    }
    for i in 0..10 {
        let result = hub.publish("t", i);
        // everyone but the second reads at once
        for (n, (_, q)) in queues.iter_mut().enumerate() {
            if n != 1 {
                check!(format!("reader {n}, message {i}"), q.try_recv().ok(), Some(i));
            }
        }
        match i {
            0..=2 => check!(format!("message {i}"), result, report(5, &[])),
            3 => check!("the fourth", result, report(4, &[queues[1].0])),
            _ => check!(format!("message {i}"), result, report(4, &[])),
        }
    }
}

#[test]
fn several_evictions_come_in_id_order() {
    let mut hub = Hub::new();
    let ids: Vec<(MemberId, Receiver<()>)> = (0..20).map(|_| hub.join(1)).collect();
    for (id, _) in ids.iter().rev() {
        hub.subscribe(*id, "t");
    }
    hub.publish("t", ());
    let mut want: Vec<MemberId> = ids.iter().map(|(id, _)| *id).collect();
    want.sort();
    check!("all full", hub.publish("t", ()), report(0, &want));
}

#[test]
fn a_full_queue_that_was_read_makes_room_again() {
    let mut hub = Hub::new();
    let (a, mut q) = hub.join(1);
    hub.subscribe(a, "t");
    for i in 0..100 {
        check!(format!("{i}"), hub.publish("t", i), report(1, &[]));
        check!(format!("{i}"), q.try_recv().ok(), Some(i));
    }
}

#[test]
fn messages_are_not_copied_to_those_who_do_not_listen() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    #[derive(Debug)]
    struct Counted(Arc<AtomicUsize>);
    impl Clone for Counted {
        fn clone(&self) -> Counted {
            self.0.fetch_add(1, Ordering::SeqCst);
            Counted(self.0.clone())
        }
    }
    let copies = Arc::new(AtomicUsize::new(0));
    let mut hub = Hub::new();
    let queues: Vec<_> = (0..10).map(|_| hub.join(4)).collect();
    for (id, _) in queues.iter().take(3) {
        hub.subscribe(*id, "t");
    }
    hub.publish("t", Counted(copies.clone()));
    check!(format!("{} clones for 3 listeners", copies.load(Ordering::SeqCst)), copies.load(Ordering::SeqCst) <= 3, true);
}

#[test]
fn random_use_matches_a_model() {
    use std::collections::{BTreeMap, BTreeSet, VecDeque};
    struct Model {
        capacity: usize,
        topics: BTreeSet<&'static str>,
        queued: VecDeque<usize>,
    }
    let mut rng = anneal_prelude::Rng::new(0x3109);
    let topics = ["a", "b", "c"];
    for run in 0..300 {
        let mut hub = Hub::new();
        let mut model: BTreeMap<MemberId, Model> = BTreeMap::new();
        let mut queues: BTreeMap<MemberId, Receiver<usize>> = BTreeMap::new();
        let mut history = Vec::new();
        for step in 0..60 {
            let ids: Vec<MemberId> = queues.keys().copied().collect();
            let some = (!ids.is_empty()).then(|| *rng.pick(&ids));
            match (rng.below(7), some) {
                (0, _) | (_, None) => {
                    let capacity = rng.below(3) + 1;
                    let (id, q) = hub.join(capacity);
                    history.push(format!("join {capacity} -> {id}"));
                    queues.insert(id, q);
                    model.insert(id, Model { capacity, topics: BTreeSet::new(), queued: VecDeque::new() });
                }
                (1 | 2, Some(id)) => {
                    let topic = *rng.pick(&topics);
                    let add = rng.below(3) > 0;
                    history.push(format!("{} {id} {topic}", if add { "subscribe" } else { "unsubscribe" }));
                    let want = model.get_mut(&id).map(|m| {
                        if add { m.topics.insert(topic); } else { m.topics.remove(topic); }
                        m.topics.len()
                    });
                    let got = if add { hub.subscribe(id, topic) } else { hub.unsubscribe(id, topic) };
                    check!(format!("run {run}: {history:?}"), got, want);
                }
                (3, Some(id)) => {
                    history.push(format!("leave {id}"));
                    check!(format!("run {run}: {history:?}"), hub.leave(id), model.remove(&id).is_some());
                }
                (4, Some(id)) => {
                    history.push(format!("read {id}"));
                    let got = queues.get_mut(&id).and_then(|q| q.try_recv().ok());
                    if let Some(m) = model.get_mut(&id) {
                        check!(format!("run {run}: {history:?}"), got, m.queued.pop_front());
                    }
                }
                (_, Some(_)) => {
                    let topic = *rng.pick(&topics);
                    history.push(format!("publish {topic} {step}"));
                    let (mut delivered, mut evicted) = (0, Vec::new());
                    for (&id, m) in model.iter_mut().filter(|(_, m)| m.topics.contains(topic)) {
                        if m.queued.len() < m.capacity {
                            m.queued.push_back(step);
                            delivered += 1;
                        } else {
                            evicted.push(id);
                        }
                    }
                    for id in &evicted {
                        model.remove(id);
                    }
                    check!(format!("run {run}: {history:?}"), hub.publish(topic, step), report(delivered, &evicted));
                }
            }
            let used: BTreeSet<&str> = model.values().flat_map(|m| m.topics.iter().copied()).collect();
            check!(format!("run {run}: members and topics after {history:?}"), (hub.members(), hub.topics()), (model.len(), used.len()));
        }
    }
}

#[test]
fn a_crowded_hub_stays_fast() {
    // publishing must not look at members of other topics
    let mut hub = Hub::new();
    let n = 100_000;
    let mut members: Vec<(MemberId, Receiver<usize>)> = Vec::with_capacity(n);
    for i in 0..n {
        let (id, q) = hub.join(1);
        hub.subscribe(id, &format!("own{i}"));
        members.push((id, q));
    }
    for (i, (_, q)) in members.iter_mut().enumerate() {
        check!("publish", hub.publish(&format!("own{i}"), i), report(1, &[]));
        check!("read", q.try_recv().ok(), Some(i));
    }
    for (id, _) in &members {
        hub.leave(*id);
    }
    check!("empty", (hub.members(), hub.topics()), (0, 0));
}
